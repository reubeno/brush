#[allow(clippy::unused_async)]
pub async fn read_stdin_timeout(
    buf: &mut [u8],
    timeout: std::time::Duration,
) -> std::io::Result<usize> {
    #[cfg(unix)]
    {
        read_poll(std::io::stdin(), buf, timeout)
    }
    #[cfg(windows)]
    {
        use futures::AsyncReadExt as _;
        let mut stdin = win::AsyncStdin::open()?;
        tokio::time::timeout(timeout, stdin.read(buf))
            .await
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timed out waiting for stdin to be ready",
                )
            })?
    }
}

#[cfg(unix)]
fn read_poll<F: std::os::fd::AsFd + std::io::Read>(
    mut f: F,
    buf: &mut [u8],
    timeout: std::time::Duration,
) -> std::io::Result<usize> {
    use nix::poll;
    use nix::poll::PollFlags;
    use nix::poll::PollTimeout;
    let secs = i32::try_from(std::cmp::min(timeout.as_secs(), i32::MAX as u64)).unwrap();
    let nanos = i32::try_from(timeout.subsec_nanos()).unwrap();
    let timeout =
        PollTimeout::try_from(secs.saturating_mul(1_000).saturating_add(nanos / 1_000_000))
            .unwrap();
    let pfd = &mut [poll::PollFd::new(f.as_fd(), PollFlags::POLLIN)];
    let rtn =
        poll::poll(pfd, timeout).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    if rtn == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out waiting for fd to be ready",
        ));
    }
    f.read(buf)
}

#[cfg(windows)]
mod win {
    use futures::{AsyncRead, FutureExt};
    use std::{
        cell::RefCell,
        pin::Pin,
        sync::{Arc, Weak},
        task::{Context, Poll},
    };
    use windows::{
        core::Error,
        Win32::{
            Foundation::{
                CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, NO_ERROR,
                WIN32_ERROR,
            },
            Storage::FileSystem::{
                CreateFileW, ReadFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING,
            },
            System::Console::{
                GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
            },
            System::IO::{BindIoCompletionCallback, CancelIoEx, OVERLAPPED},
        },
    };

    #[repr(C)]
    struct Overlapped {
        // WARN: This must be the first field, with no offset, so Windows will recognize it as
        // OVERLAPPED (C-style OOP).
        overlapped: OVERLAPPED,
        tx: RefCell<Option<tokio::sync::oneshot::Sender<Result<u32, Error>>>>,
    }
    // SAFETY: We implement this because `OVERLAPPED` hasn't implemented it. We use `OVERLAPPED`
    // only inside FFI [`iocp_callback`], so it doesn't actually need to be `Send`.
    // TODO: it will be cool to get rid of this.
    // error : `*mut c_void` cannot be sent between threads safely.
    unsafe impl Send for Overlapped {}
    unsafe impl Sync for Overlapped {}

    impl Overlapped {
        pub fn new(tx: tokio::sync::oneshot::Sender<Result<u32, Error>>) -> Self {
            Overlapped {
                overlapped: OVERLAPPED::default(),
                // RefCell because we need this to be mutable inside `io_callback`, but a Mutex is
                // to much...
                tx: RefCell::new(Some(tx)),
            }
        }
    }

    pub struct AsyncStdin {
        fd: HANDLE,
        // This is Some when we have started reading, and None otherwise.
        rx: Option<tokio::sync::oneshot::Receiver<Result<u32, Error>>>,
        // keep ownership of the [`iocp_callback`] payload.
        // Arc instead of Rc, because our AsyncStdin needs to be Send
        iocp_payload: Option<Arc<Overlapped>>,
    }

    impl AsyncStdin {
        pub fn open() -> Result<Self, Error> {
            // open our stdin in the async mode
            let stdin_handle = unsafe {
                CreateFileW(
                    windows::core::w!("CONIN$"),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    None,
                )?
            };
            let mut console_mode = CONSOLE_MODE::default();

            // SAFETY: win32 api calls.

            // TODO: Document that this mode applies only to the currently opened handle,
            // so there's no need to revert it inside `Drop`.
            unsafe { GetConsoleMode(stdin_handle, &mut console_mode) }?;
            // Flush on each character instead of only at line endings (`\n`).

            // disable ENABLE_LINE_INPUT for the read operation to return when the operation is
            // complete instead of when we hit enter.
            // NOTE: Can't use `ENABLE_ECHO_INPUT` without `ENABLE_LINE_INPUT`.
            // TODO: We need to print read characters to emulate standard behavior. Or refactor our
            // read logic to handle C-C/C-D differently.
            console_mode = console_mode & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT);
            unsafe { SetConsoleMode(stdin_handle, console_mode) }?;

            unsafe { BindIoCompletionCallback(stdin_handle, Some(iocp_callback), 0) }?;

            Ok(Self {
                fd: stdin_handle,
                rx: None,
                iocp_payload: None,
            })
        }
    }

    impl Drop for AsyncStdin {
        fn drop(&mut self) {
            unsafe {
                // if it is Some, we have pending operations
                if let Some(iocp) = &self.iocp_payload {
                    // Can be already completed somehow..
                    let _ = CancelIoEx(self.fd, Some(&iocp.overlapped as _));
                }
                CloseHandle(self.fd).unwrap();
            }
        }
    }

    impl AsyncRead for AsyncStdin {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            let result = if let Some(rx) = &mut self.rx {
                rx.poll_unpin(cx)
            } else {
                // This is the first time `poll` is called
                let (tx, rx) = tokio::sync::oneshot::channel();

                self.rx = Some(rx);

                // The OVERLAPPED data structure must remain valid for the duration of the read
                // operation. It should not be a variable that can go out of scope while the read
                // operation is pending completion.
                let tx = Arc::new(Overlapped::new(tx));
                let weak = Arc::downgrade(&tx);
                self.iocp_payload = Some(tx);

                // it will be recreated inside the callback
                let overlapped = {
                    let raw: *const Overlapped = weak.as_ptr();
                    // OOP here! cast to the "parent" struct - the first field in the struct with
                    // the C-layout
                    raw as *mut OVERLAPPED
                };

                let ok = unsafe { ReadFile(self.fd, Some(buf), None, Some(overlapped as _)) };
                match ok {
                    Err(e) => {
                        if e == Error::from(ERROR_IO_PENDING) || e == Error::from(NO_ERROR) {
                            // case: callback is invoked.
                            self.rx.as_mut().unwrap().poll_unpin(cx)
                        } else {
                            // case: error.
                            // > The callback function might not be executed if the process issues an
                            // asynchronous request on the file specified by the FileHandle
                            // parameter but the request returns immediately with an error code
                            // other than ERROR_IO_PENDING.
                            Poll::Ready(Ok(Err(e)))
                        }
                    }
                    Ok(()) => {
                        // ok: callback is invoked. operation completed synchronously
                        self.rx.as_mut().unwrap().poll_unpin(cx)
                    }
                }
            };
            // we are ready, lets cleanup everything
            if result.is_ready() {
                self.rx = None;
                self.iocp_payload = None;
            }
            result
                .map(|r| r.unwrap().map(|n| n as usize))
                .map_err(|e| std::io::Error::from(e))
        }
    }

    // https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/nc-minwinbase-lpoverlapped_completion_routine
    unsafe extern "system" fn iocp_callback(
        dwerrorcode: u32,
        dwnumberofbytestransfered: u32,
        lpoverlapped: *mut OVERLAPPED,
    ) {
        let e = Error::from(WIN32_ERROR(dwerrorcode));

        let result = {
            // NOTE: code 0 is not and error
            if e.code().is_err() {
                Err(e)
            } else {
                Ok(dwnumberofbytestransfered)
            }
        };
        // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-bindiocompletioncallback
        // > The system does not use the OVERLAPPED structure after the completion routine is
        // > called, so the completion routine can deallocate the memory used by the overlapped
        // > structure.

        // NOTE: The callback is called exactly once, but for safety reasons, to prevent memory
        // leaks in case the callback is not called, `AsyncStdin` holds ownership of the
        // `Overlapped` struct anyway.

        let tx = Weak::from_raw(lpoverlapped as *mut Overlapped);
        if let Some(tx) = tx.upgrade() {
            let _ = tx.tx.take().unwrap().send(result);
        }
    }
}
