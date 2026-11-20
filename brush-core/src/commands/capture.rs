//! Collect a stream's output without letting the reader task own its storage.
//!
//! The caller and worker share an in-process buffer, not a pipe or IPC channel.
//! The input itself may come from a file, an external process, or another task.
//!
//! A shell operation can be cancelled while a worker is blocked reading input.
//! Dropping its task handle does not stop that worker. The awaiting caller must
//! therefore own the output buffer, so cancelling it frees the accumulated bytes
//! rather than leaving them in an abandoned worker.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, Weak};

use crate::{error, extensions::TaskSpawner, ioutils};

/// Reads input on a worker while keeping the collected bytes owned by this future.
///
/// The input uses synchronous `Read`, which can block waiting for a file or pipe.
/// On native runtimes, a blocking worker keeps that wait off the executor threads
/// needed to run producers and other tasks. On WASM, the worker runs cooperatively
/// instead; it does not make blocking I/O safe.
///
/// We collect directly into memory instead of forwarding through another pipe:
/// that pipe could fill while its reader's executor is blocked by the producer.
/// Dropping this future releases the collected bytes and asks the worker to stop;
/// a read already blocked in the worker can still outlive it.
pub(super) async fn collect_string_on_worker(
    input: impl Read + Send + 'static,
    spawner: &impl TaskSpawner,
) -> Result<String, error::Error> {
    let output = Buffer::default();
    let copy_task = spawner.spawn_blocking(output.copy_from(input)).await;
    let copy_result = copy_task.await?;
    copy_result?;
    Ok(output.into_string()?)
}

/// A byte buffer owned by the operation awaiting the result, not its worker.
///
/// `Arc<Mutex<Vec<u8>>>` provides shared access across threads. The writer holds
/// a `Weak` reference: it can borrow the buffer for one append, but cannot keep
/// its bytes alive while waiting for input. Dropping this owner releases the
/// bytes once any append already in progress finishes. The reader and writer
/// stop copying when they next observe that the owner is gone.
///
/// Cancellation can race with the check before a read, so one last read may
/// still block. We cannot interrupt it, but it does not retain the output buffer.
#[derive(Default)]
struct Buffer {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Buffer {
    /// Builds a copier that borrows storage only while appending, not while reading.
    ///
    /// The returned future does not borrow `self`, so it can outlive the owner.
    fn copy_from<R: Read>(&self, input: R) -> impl Future<Output = io::Result<()>> + use<R> {
        let reader = Reader {
            input,
            destination: Arc::downgrade(&self.bytes),
        };
        let writer = Writer {
            destination: Arc::downgrade(&self.bytes),
        };
        ioutils::cooperative_copy(reader, writer)
    }

    /// Takes the completed output, without cloning its bytes.
    fn into_string(self) -> io::Result<String> {
        // The worker has finished before this is called. A remaining strong
        // reference would mean a writer is still using the output.
        let output = Arc::try_unwrap(self.bytes)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::ResourceBusy,
                    "output collection has not finished",
                )
            })?
            .into_inner()
            // A poisoned mutex is an internal collection failure, not a specific
            // filesystem error, so Other is the appropriate I/O error kind.
            .map_err(|_| io::Error::other("could not retrieve collected output"))?;
        String::from_utf8(output).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

/// Appends bytes only while the operation that requested them is still alive.
struct Writer {
    destination: Weak<Mutex<Vec<u8>>>,
}

/// Stops the copier before its next read if the destination has been abandoned.
struct Reader<R> {
    input: R,
    destination: Weak<Mutex<Vec<u8>>>,
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        // Check before every attempt, including retries after Interrupted. A writer
        // check alone would miss input that repeatedly fails without producing bytes.
        // Do not upgrade to Arc here: it would retain the buffer during a blocked read.
        if self.destination.strong_count() == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.input.read(buffer)
    }
}

impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        // Keep the buffer alive only for this append. BrokenPipe tells the copier
        // to stop when the caller has dropped its destination.
        let output = self
            .destination
            .upgrade()
            .ok_or(io::ErrorKind::BrokenPipe)?;
        output
            .lock()
            .map_err(|_| io::Error::other("could not append collected output"))?
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Each write appends directly to the destination; nothing is buffered here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions;

    #[tokio::test]
    async fn substitution_input_rejects_invalid_utf8() -> anyhow::Result<()> {
        let output = Buffer::default();
        output.copy_from(&b"\xff"[..]).await?;
        let result = output.into_string();
        anyhow::ensure!(
            result
                .as_ref()
                .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidData),
            "unexpected result: {result:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn substitution_input_cancels_while_waiting_for_cooperative_input() -> anyhow::Result<()>
    {
        struct Unready(Option<tokio::sync::oneshot::Sender<()>>);

        impl std::io::Read for Unready {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                if let Some(started) = self.0.take() {
                    let _ = started.send(());
                }
                Err(std::io::ErrorKind::WouldBlock.into())
            }
        }

        let output = Buffer::default();
        let (started, has_started) = tokio::sync::oneshot::channel();
        let reader = tokio::spawn(output.copy_from(Unready(Some(started))));
        tokio::time::timeout(std::time::Duration::from_secs(2), has_started).await??;
        drop(output);
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), reader).await??;
        anyhow::ensure!(
            result
                .as_ref()
                .is_err_and(|err| err.kind() == std::io::ErrorKind::BrokenPipe),
            "unexpected result: {result:?}"
        );
        Ok(())
    }

    #[cfg(any(unix, windows))] // ast-grep-ignore: platform-cfg-outside-sys
    #[tokio::test(flavor = "multi_thread")]
    async fn cancelled_substitution_releases_output_while_read_is_blocked() -> anyhow::Result<()> {
        struct PausedInput {
            first: bool,
            started: Option<tokio::sync::oneshot::Sender<()>>,
            resume: std::sync::mpsc::Receiver<()>,
        }

        impl std::io::Read for PausedInput {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.first {
                    self.first = false;
                    return (&b"first"[..]).read(buffer);
                }
                if let Some(started) = self.started.take() {
                    let _ = started.send(());
                }
                self.resume.recv().map_err(std::io::Error::other)?;
                (&b"later"[..]).read(buffer)
            }
        }

        let output = Buffer::default();
        let observer = Arc::downgrade(&output.bytes);
        let (started, has_started) = tokio::sync::oneshot::channel();
        let (resume, wait_for_resume) = std::sync::mpsc::channel();
        let input = PausedInput {
            first: true,
            started: Some(started),
            resume: wait_for_resume,
        };
        let reader = extensions::DefaultTaskSpawner
            .spawn_blocking(output.copy_from(input))
            .await;

        tokio::time::timeout(std::time::Duration::from_secs(2), has_started).await??;
        let captured = output.bytes.try_lock().ok().map(|buffer| buffer.clone());
        drop(output);
        let released = observer.upgrade().is_none();
        resume.send(())?;
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), reader).await??;

        anyhow::ensure!(
            captured.as_deref() == Some(&b"first"[..]),
            "unexpected captured bytes: {captured:?}"
        );
        anyhow::ensure!(released, "input capture was retained by the blocked reader");
        anyhow::ensure!(
            result
                .as_ref()
                .is_err_and(|err| err.kind() == std::io::ErrorKind::BrokenPipe),
            "unexpected result: {result:?}"
        );
        Ok(())
    }

    #[cfg(any(unix, windows))] // ast-grep-ignore: platform-cfg-outside-sys
    #[tokio::test(flavor = "multi_thread")]
    async fn cancelled_substitution_does_not_retry_interrupted_read() -> anyhow::Result<()> {
        struct InterruptedInput {
            started: Option<tokio::sync::oneshot::Sender<()>>,
            resume: std::sync::mpsc::Receiver<()>,
        }

        impl std::io::Read for InterruptedInput {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                if let Some(started) = self.started.take() {
                    let _ = started.send(());
                    self.resume.recv().map_err(std::io::Error::other)?;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                Err(std::io::ErrorKind::InvalidData.into())
            }
        }

        let output = Buffer::default();
        let (started, has_started) = tokio::sync::oneshot::channel();
        let (resume, wait_for_resume) = std::sync::mpsc::channel();
        let input = InterruptedInput {
            started: Some(started),
            resume: wait_for_resume,
        };
        let reader = extensions::DefaultTaskSpawner
            .spawn_blocking(output.copy_from(input))
            .await;

        tokio::time::timeout(std::time::Duration::from_secs(2), has_started).await??;
        drop(output);
        resume.send(())?;
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), reader).await??;
        anyhow::ensure!(
            result
                .as_ref()
                .is_err_and(|err| err.kind() == std::io::ErrorKind::BrokenPipe),
            "unexpected result: {result:?}"
        );
        Ok(())
    }
}
