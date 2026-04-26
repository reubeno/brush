//! Managing files open within a shell instance.

use crate::error;
use crate::sys;

/// Blocking file types for child process spawning.
///
/// These types are used internally for converting to `std::process::Stdio`
/// when spawning child processes.
pub mod blocking {
    use std::io::IsTerminal;
    use std::process::Stdio;

    use crate::ShellFd;
    use crate::error;
    use crate::ioutils;

    /// A trait representing a blocking stream that can be read from and written to.
    pub trait BlockingStream: std::io::Read + std::io::Write + Send + Sync {
        /// Clones the stream into a boxed trait object.
        fn clone_box(&self) -> Box<dyn BlockingStream>;

        /// Converts the stream into an `OwnedFd`.
        #[cfg(unix)]
        fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, error::Error>;

        /// Borrows the stream as a `BorrowedFd`.
        #[cfg(unix)]
        fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error>;
    }

    /// Represents a blocking file open in a shell context.
    pub enum File {
        /// The original standard input this process was started with.
        Stdin(std::io::Stdin),
        /// The original standard output this process was started with.
        Stdout(std::io::Stdout),
        /// The original standard error this process was started with.
        Stderr(std::io::Stderr),
        /// A file open for reading or writing.
        File(std::fs::File),
        /// A read end of a pipe.
        PipeReader(std::io::PipeReader),
        /// A write end of a pipe.
        PipeWriter(std::io::PipeWriter),
        /// A custom stream.
        Stream(Box<dyn BlockingStream>),
    }

    impl Clone for File {
        fn clone(&self) -> Self {
            self.try_clone().unwrap_or_else(|_err| {
                ioutils::FailingReaderWriter::new("failed to duplicate open file").into()
            })
        }
    }

    impl std::fmt::Display for File {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Stdin(_) => write!(f, "stdin"),
                Self::Stdout(_) => write!(f, "stdout"),
                Self::Stderr(_) => write!(f, "stderr"),
                Self::File(_) => write!(f, "file"),
                Self::PipeReader(_) => write!(f, "pipe reader"),
                Self::PipeWriter(_) => write!(f, "pipe writer"),
                Self::Stream(_) => write!(f, "stream"),
            }
        }
    }

    impl File {
        /// Tries to duplicate the open file.
        pub fn try_clone(&self) -> Result<Self, std::io::Error> {
            let result = match self {
                Self::Stdin(_) => std::io::stdin().into(),
                Self::Stdout(_) => std::io::stdout().into(),
                Self::Stderr(_) => std::io::stderr().into(),
                Self::File(f) => f.try_clone()?.into(),
                Self::PipeReader(f) => f.try_clone()?.into(),
                Self::PipeWriter(f) => f.try_clone()?.into(),
                Self::Stream(s) => Self::Stream(s.clone_box()),
            };

            Ok(result)
        }

        /// Converts the open file into an `OwnedFd`.
        #[cfg(unix)]
        #[allow(dead_code)]
        pub(crate) fn try_clone_to_owned(self) -> Result<std::os::fd::OwnedFd, error::Error> {
            use std::os::fd::AsFd as _;

            match self {
                Self::Stdin(f) => Ok(f.as_fd().try_clone_to_owned()?),
                Self::Stdout(f) => Ok(f.as_fd().try_clone_to_owned()?),
                Self::Stderr(f) => Ok(f.as_fd().try_clone_to_owned()?),
                Self::File(f) => Ok(f.into()),
                Self::PipeReader(r) => Ok(std::os::fd::OwnedFd::from(r)),
                Self::PipeWriter(w) => Ok(std::os::fd::OwnedFd::from(w)),
                Self::Stream(s) => s.try_clone_to_owned(),
            }
        }

        /// Borrows the open file as a `BorrowedFd`.
        #[cfg(unix)]
        pub fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error> {
            use std::os::fd::AsFd as _;

            match self {
                Self::Stdin(f) => Ok(f.as_fd()),
                Self::Stdout(f) => Ok(f.as_fd()),
                Self::Stderr(f) => Ok(f.as_fd()),
                Self::File(f) => Ok(f.as_fd()),
                Self::PipeReader(r) => Ok(r.as_fd()),
                Self::PipeWriter(w) => Ok(w.as_fd()),
                Self::Stream(s) => s.try_borrow_as_fd(),
            }
        }

        #[allow(dead_code)]
        pub(crate) fn is_dir(&self) -> bool {
            match self {
                Self::Stdin(_) | Self::Stdout(_) | Self::Stderr(_) => false,
                Self::File(file) => file.metadata().is_ok_and(|m| m.is_dir()),
                Self::PipeReader(_) | Self::PipeWriter(_) | Self::Stream(_) => false,
            }
        }

        /// Checks if the open file is associated with a terminal.
        pub fn is_terminal(&self) -> bool {
            match self {
                Self::Stdin(f) => f.is_terminal(),
                Self::Stdout(f) => f.is_terminal(),
                Self::Stderr(f) => f.is_terminal(),
                Self::File(f) => f.is_terminal(),
                Self::PipeReader(_) | Self::PipeWriter(_) | Self::Stream(_) => false,
            }
        }
    }

    impl From<std::io::Stdin> for File {
        fn from(stdin: std::io::Stdin) -> Self {
            Self::Stdin(stdin)
        }
    }

    impl From<std::io::Stdout> for File {
        fn from(stdout: std::io::Stdout) -> Self {
            Self::Stdout(stdout)
        }
    }

    impl From<std::io::Stderr> for File {
        fn from(stderr: std::io::Stderr) -> Self {
            Self::Stderr(stderr)
        }
    }

    impl From<std::fs::File> for File {
        fn from(file: std::fs::File) -> Self {
            Self::File(file)
        }
    }

    impl From<std::io::PipeReader> for File {
        fn from(reader: std::io::PipeReader) -> Self {
            Self::PipeReader(reader)
        }
    }

    impl From<std::io::PipeWriter> for File {
        fn from(writer: std::io::PipeWriter) -> Self {
            Self::PipeWriter(writer)
        }
    }

    impl From<File> for Stdio {
        fn from(open_file: File) -> Self {
            match open_file {
                File::Stdin(_) => Self::inherit(),
                File::Stdout(_) => Self::inherit(),
                File::Stderr(_) => Self::inherit(),
                File::File(f) => f.into(),
                File::PipeReader(f) => f.into(),
                File::PipeWriter(f) => f.into(),
                File::Stream(_) => Self::null(),
            }
        }
    }

    impl std::io::Read for File {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self {
                Self::Stdin(f) => f.read(buf),
                Self::Stdout(_) => Err(std::io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("stdout"),
                )),
                Self::Stderr(_) => Err(std::io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("stderr"),
                )),
                Self::File(f) => f.read(buf),
                Self::PipeReader(reader) => reader.read(buf),
                Self::PipeWriter(_) => Err(std::io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("pipe writer"),
                )),
                Self::Stream(s) => s.read(buf),
            }
        }
    }

    impl std::io::Write for File {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self {
                Self::Stdin(_) => Err(std::io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("stdin"),
                )),
                Self::Stdout(f) => f.write(buf),
                Self::Stderr(f) => f.write(buf),
                Self::File(f) => f.write(buf),
                Self::PipeReader(_) => Err(std::io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("pipe reader"),
                )),
                Self::PipeWriter(writer) => writer.write(buf),
                Self::Stream(s) => s.write(buf),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Self::Stdin(_) => Ok(()),
                Self::Stdout(f) => f.flush(),
                Self::Stderr(f) => f.flush(),
                Self::File(f) => f.flush(),
                Self::PipeReader(_) => Ok(()),
                Self::PipeWriter(writer) => writer.flush(),
                Self::Stream(s) => s.flush(),
            }
        }
    }

    /// Represents the blocking open files in a shell context.
    #[derive(Clone, Default)]
    pub struct Files {
        pub(crate) files: std::collections::HashMap<ShellFd, Option<File>>,
    }

    impl Files {
        /// File descriptor for standard input.
        pub const STDIN_FD: ShellFd = 0;
        /// File descriptor for standard output.
        pub const STDOUT_FD: ShellFd = 1;
        /// File descriptor for standard error.
        pub const STDERR_FD: ShellFd = 2;

        #[allow(dead_code)]
        pub(crate) fn new() -> Self {
            Self {
                files: std::collections::HashMap::from([
                    (Self::STDIN_FD, Some(std::io::stdin().into())),
                    (Self::STDOUT_FD, Some(std::io::stdout().into())),
                    (Self::STDERR_FD, Some(std::io::stderr().into())),
                ]),
            }
        }

        /// Returns a reference to the standard input file, if available.
        pub fn try_stdin(&self) -> Option<&File> {
            self.files.get(&Self::STDIN_FD).and_then(|f| f.as_ref())
        }

        /// Returns a reference to the standard output file, if available.
        pub fn try_stdout(&self) -> Option<&File> {
            self.files.get(&Self::STDOUT_FD).and_then(|f| f.as_ref())
        }

        /// Returns a reference to the standard error file, if available.
        pub fn try_stderr(&self) -> Option<&File> {
            self.files.get(&Self::STDERR_FD).and_then(|f| f.as_ref())
        }

        /// Returns a reference to the file at the given file descriptor, if available.
        pub fn try_fd(&self, fd: ShellFd) -> Option<&File> {
            self.files.get(&fd).and_then(|f| f.as_ref())
        }

        /// Sets the file at the given file descriptor, returning the previous file if any.
        pub fn set_fd(&mut self, fd: ShellFd, file: File) -> Option<File> {
            self.files.insert(fd, Some(file)).and_then(|f| f)
        }

        /// Adds a new file, returning the assigned file descriptor.
        pub fn add(&mut self, file: File) -> Result<ShellFd, error::Error> {
            let mut fd = 3;
            while self.files.contains_key(&fd) {
                if fd >= 1024 {
                    return Err(error::ErrorKind::TooManyOpenFiles.into());
                }
                fd += 1;
            }
            self.files.insert(fd, Some(file));
            Ok(fd)
        }

        /// Removes the file at the given file descriptor, returning it if it existed.
        pub fn remove_fd(&mut self, fd: ShellFd) -> Option<File> {
            self.files.insert(fd, None).and_then(|f| f)
        }

        /// Returns an iterator over all file descriptors and their associated files.
        pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, &File)> {
            self.files
                .iter()
                .filter_map(|(fd, file)| file.as_ref().map(|f| (*fd, f)))
        }
    }

    impl<I> From<I> for Files
    where
        I: Iterator<Item = (ShellFd, File)>,
    {
        fn from(iter: I) -> Self {
            let files = iter.map(|(fd, file)| (fd, Some(file))).collect();
            Self { files }
        }
    }
}

/// Returns an open file that will discard all I/O.
pub fn null() -> Result<File, error::Error> {
    let file = sys::fs::open_null_file()?;
    Ok(File::File(file))
}

pub use blocking::{BlockingStream, File};

/// Async file abstractions for non-blocking I/O operations.
pub mod async_file {
    use std::io::{self, IsTerminal};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use crate::error;

    /// Polyfill for tokio's stdio and file types on wasm targets.
    ///
    /// Since wasm targets don't support tokio's `io-std` and `fs` features,
    /// we provide blocking wrappers that implement the async traits.
    #[cfg(target_family = "wasm")]
    pub mod stdio_polyfill {
        use std::io::{self, IsTerminal, Read as _, Write as _};
        use std::pin::Pin;
        use std::task::{Context, Poll};

        use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

        /// Async wrapper for standard input on wasm.
        pub struct Stdin(io::Stdin);

        /// Async wrapper for standard output on wasm.
        pub struct Stdout(io::Stdout);

        /// Async wrapper for standard error on wasm.
        pub struct Stderr(io::Stderr);

        /// Async wrapper for a file on wasm.
        pub struct File(std::fs::File);

        impl File {
            /// Creates a new async file from a standard file.
            pub fn from_std(file: std::fs::File) -> Self {
                Self(file)
            }

            /// Tries to convert the async file back to a standard file.
            pub fn try_into_std(self) -> Result<std::fs::File, Self> {
                Ok(self.0)
            }

            /// Returns metadata about the file.
            pub async fn metadata(&self) -> io::Result<std::fs::Metadata> {
                self.0.metadata()
            }

            /// Creates a new independently owned handle to the underlying file.
            pub fn try_clone(&self) -> io::Result<Self> {
                self.0.try_clone().map(Self)
            }
        }

        impl AsyncRead for File {
            fn poll_read(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                let n = self.0.read(buf.initialize_unfilled())?;
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
        }

        impl AsyncWrite for File {
            fn poll_write(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Poll::Ready(self.0.write(buf))
            }

            fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(self.0.flush())
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                self.poll_flush(cx)
            }
        }

        impl Stdin {
            /// Creates a new async stdin wrapper.
            pub fn new() -> Self {
                Self(io::stdin())
            }
        }

        impl Stdout {
            /// Creates a new async stdout wrapper.
            pub fn new() -> Self {
                Self(io::stdout())
            }
        }

        impl Stderr {
            /// Creates a new async stderr wrapper.
            pub fn new() -> Self {
                Self(io::stderr())
            }
        }

        impl AsyncRead for Stdin {
            fn poll_read(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                let n = self.0.read(buf.initialize_unfilled())?;
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
        }

        impl AsyncWrite for Stdout {
            fn poll_write(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Poll::Ready(self.0.write(buf))
            }

            fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(self.0.flush())
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                self.poll_flush(cx)
            }
        }

        impl AsyncWrite for Stderr {
            fn poll_write(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Poll::Ready(self.0.write(buf))
            }

            fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(self.0.flush())
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                self.poll_flush(cx)
            }
        }

        impl Stdin {
            /// Returns true if this is a terminal.
            pub fn is_terminal(&self) -> bool {
                self.0.is_terminal()
            }
        }

        impl Stdout {
            /// Returns true if this is a terminal.
            pub fn is_terminal(&self) -> bool {
                self.0.is_terminal()
            }
        }

        impl Stderr {
            /// Returns true if this is a terminal.
            pub fn is_terminal(&self) -> bool {
                self.0.is_terminal()
            }
        }
    }

    #[cfg(target_family = "wasm")]
    use stdio_polyfill::{File, Stderr, Stdin, Stdout};

    #[cfg(not(target_family = "wasm"))]
    use tokio::fs::File;

    #[cfg(not(target_family = "wasm"))]
    use tokio::io::{Stderr, Stdin, Stdout};

    #[cfg(target_family = "wasm")]
    fn stdin() -> Stdin {
        Stdin::new()
    }

    #[cfg(target_family = "wasm")]
    fn stdout() -> Stdout {
        Stdout::new()
    }

    #[cfg(target_family = "wasm")]
    fn stderr() -> Stderr {
        Stderr::new()
    }

    #[cfg(not(target_family = "wasm"))]
    fn stdin() -> Stdin {
        tokio::io::stdin()
    }

    #[cfg(not(target_family = "wasm"))]
    fn stdout() -> Stdout {
        tokio::io::stdout()
    }

    #[cfg(not(target_family = "wasm"))]
    fn stderr() -> Stderr {
        tokio::io::stderr()
    }

    /// Sets a file descriptor to blocking mode.
    #[cfg(unix)]
    fn set_fd_blocking(fd: &impl std::os::fd::AsFd) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        let borrowed_fd = fd.as_fd();
        let raw_fd = borrowed_fd.as_raw_fd();
        // SAFETY: fcntl with F_GETFL is safe to call on a valid file descriptor.
        let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: fcntl with F_SETFL is safe to call on a valid file descriptor with valid flags.
        let result = unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// A trait representing an async stream that can be read from and written to.
    pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {
        /// Clones the stream into a boxed trait object.
        fn clone_box(&self) -> Box<dyn AsyncStream>;

        /// Converts the stream into an `OwnedFd`.
        #[cfg(unix)]
        fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, error::Error>;

        /// Borrows the stream as a `BorrowedFd`.
        #[cfg(unix)]
        fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error>;
    }

    /// Represents an async file open in a shell context.
    #[cfg(unix)]
    pub enum AsyncOpenFile {
        /// The original standard input.
        Stdin(Stdin),
        /// The original standard output.
        Stdout(Stdout),
        /// The original standard error.
        Stderr(Stderr),
        /// A file open for reading or writing.
        File(File),
        /// The read end of a pipe.
        PipeReader(tokio::net::unix::pipe::Receiver),
        /// The write end of a pipe.
        PipeWriter(tokio::net::unix::pipe::Sender),
        /// A custom async stream.
        Stream(Box<dyn AsyncStream>),
        /// A broken file that always returns an error; used as a safe fallback when a
        /// file cannot be cloned or converted instead of silently misdirecting I/O.
        Broken(String),
    }

    /// Represents an async file open in a shell context.
    #[cfg(not(unix))]
    pub enum AsyncOpenFile {
        /// The original standard input.
        Stdin(Stdin),
        /// The original standard output.
        Stdout(Stdout),
        /// The original standard error.
        Stderr(Stderr),
        /// A file open for reading or writing.
        File(File),
        /// The read end of a pipe.
        PipeReader(tokio::io::DuplexStream),
        /// The write end of a pipe.
        PipeWriter(tokio::io::DuplexStream),
        /// A custom async stream.
        Stream(Box<dyn AsyncStream>),
        /// A broken file that always returns an error; used as a safe fallback when a
        /// file cannot be cloned or converted instead of silently misdirecting I/O.
        Broken(String),
    }

    impl AsyncRead for AsyncOpenFile {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            match self.get_mut() {
                #[cfg(unix)]
                Self::Stdin(f) => Pin::new(f).poll_read(cx, buf),
                #[cfg(unix)]
                Self::PipeReader(r) => Pin::new(r).poll_read(cx, buf),
                #[cfg(unix)]
                Self::PipeWriter(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("pipe writer"),
                ))),
                #[cfg(not(unix))]
                Self::Stdin(f) => Pin::new(f).poll_read(cx, buf),
                #[cfg(not(unix))]
                Self::PipeReader(r) => Pin::new(r).poll_read(cx, buf),
                #[cfg(not(unix))]
                Self::PipeWriter(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("pipe writer"),
                ))),
                Self::Stdout(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("stdout"),
                ))),
                Self::Stderr(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("stderr"),
                ))),
                Self::File(f) => Pin::new(f).poll_read(cx, buf),
                Self::Stream(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
                Self::Broken(msg) => Poll::Ready(Err(io::Error::other(msg.clone()))),
            }
        }
    }

    impl AsyncWrite for AsyncOpenFile {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.get_mut() {
                #[cfg(unix)]
                Self::Stdin(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("stdin"),
                ))),
                #[cfg(unix)]
                Self::Stdout(f) => Pin::new(f).poll_write(cx, buf),
                #[cfg(unix)]
                Self::Stderr(f) => Pin::new(f).poll_write(cx, buf),
                #[cfg(unix)]
                Self::PipeReader(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("pipe reader"),
                ))),
                #[cfg(unix)]
                Self::PipeWriter(w) => Pin::new(w).poll_write(cx, buf),
                #[cfg(not(unix))]
                Self::Stdin(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("stdin"),
                ))),
                #[cfg(not(unix))]
                Self::Stdout(f) => Pin::new(f).poll_write(cx, buf),
                #[cfg(not(unix))]
                Self::Stderr(f) => Pin::new(f).poll_write(cx, buf),
                #[cfg(not(unix))]
                Self::PipeReader(_) => Poll::Ready(Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("pipe reader"),
                ))),
                #[cfg(not(unix))]
                Self::PipeWriter(w) => Pin::new(w).poll_write(cx, buf),
                Self::File(f) => Pin::new(f).poll_write(cx, buf),
                Self::Stream(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
                Self::Broken(msg) => Poll::Ready(Err(io::Error::other(msg.clone()))),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Stdin(_) => Poll::Ready(Ok(())),
                Self::Stdout(f) => Pin::new(f).poll_flush(cx),
                Self::Stderr(f) => Pin::new(f).poll_flush(cx),
                Self::File(f) => Pin::new(f).poll_flush(cx),
                Self::PipeReader(_) => Poll::Ready(Ok(())),
                #[cfg(unix)]
                Self::PipeWriter(w) => Pin::new(w).poll_flush(cx),
                #[cfg(not(unix))]
                Self::PipeWriter(w) => Pin::new(w).poll_flush(cx),
                Self::Stream(s) => Pin::new(s.as_mut()).poll_flush(cx),
                Self::Broken(msg) => Poll::Ready(Err(io::Error::other(msg.clone()))),
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Stdin(_) => Poll::Ready(Ok(())),
                Self::Stdout(f) => Pin::new(f).poll_shutdown(cx),
                Self::Stderr(f) => Pin::new(f).poll_shutdown(cx),
                Self::File(f) => Pin::new(f).poll_shutdown(cx),
                Self::PipeReader(_) => Poll::Ready(Ok(())),
                #[cfg(unix)]
                Self::PipeWriter(w) => Pin::new(w).poll_shutdown(cx),
                #[cfg(not(unix))]
                Self::PipeWriter(w) => Pin::new(w).poll_shutdown(cx),
                Self::Stream(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
                Self::Broken(msg) => Poll::Ready(Err(io::Error::other(msg.clone()))),
            }
        }
    }

    #[cfg(unix)]
    impl AsyncOpenFile {
        /// Creates an async file from a standard file.
        pub fn from_std_file(file: std::fs::File) -> Self {
            Self::File(tokio::fs::File::from_std(file))
        }

        /// Creates an async pipe reader from a blocking pipe reader.
        pub fn from_pipe_reader(reader: std::io::PipeReader) -> io::Result<Self> {
            use std::os::fd::OwnedFd;
            let owned_fd = OwnedFd::from(reader);
            let receiver =
                tokio::net::unix::pipe::Receiver::from_file(std::fs::File::from(owned_fd))?;
            Ok(Self::PipeReader(receiver))
        }

        /// Creates an async pipe writer from a blocking pipe writer.
        pub fn from_pipe_writer(writer: std::io::PipeWriter) -> io::Result<Self> {
            use std::os::fd::OwnedFd;
            let owned_fd = OwnedFd::from(writer);
            let sender = tokio::net::unix::pipe::Sender::from_file(std::fs::File::from(owned_fd))?;
            Ok(Self::PipeWriter(sender))
        }

        /// Tries to clone the open file.
        pub fn try_clone(&self) -> io::Result<Self> {
            match self {
                Self::Stdin(_) => Ok(Self::Stdin(stdin())),
                Self::Stdout(_) => Ok(Self::Stdout(stdout())),
                Self::Stderr(_) => Ok(Self::Stderr(stderr())),
                Self::File(f) => {
                    use std::os::fd::{AsFd, FromRawFd, OwnedFd};
                    let raw =
                        nix::fcntl::fcntl(f.as_fd(), nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(0))?;
                    // SAFETY: fcntl(F_DUPFD_CLOEXEC) returns a valid, new file descriptor on success.
                    let new_fd = unsafe { OwnedFd::from_raw_fd(raw) };
                    let std_file = std::fs::File::from(new_fd);
                    Ok(Self::File(File::from_std(std_file)))
                }
                Self::PipeReader(r) => {
                    use std::os::fd::{AsFd, FromRawFd, OwnedFd};
                    let raw =
                        nix::fcntl::fcntl(r.as_fd(), nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(0))?;
                    // SAFETY: fcntl(F_DUPFD_CLOEXEC) returns a valid, new file descriptor on success.
                    let new_fd = unsafe { OwnedFd::from_raw_fd(raw) };
                    Ok(Self::PipeReader(
                        tokio::net::unix::pipe::Receiver::from_owned_fd_unchecked(new_fd)?,
                    ))
                }
                Self::PipeWriter(w) => {
                    use std::os::fd::{AsFd, FromRawFd, OwnedFd};
                    let raw =
                        nix::fcntl::fcntl(w.as_fd(), nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(0))?;
                    // SAFETY: fcntl(F_DUPFD_CLOEXEC) returns a valid, new file descriptor on success.
                    let new_fd = unsafe { OwnedFd::from_raw_fd(raw) };
                    Ok(Self::PipeWriter(
                        tokio::net::unix::pipe::Sender::from_owned_fd_unchecked(new_fd)?,
                    ))
                }
                Self::Stream(s) => Ok(Self::Stream(s.clone_box())),
                Self::Broken(msg) => Ok(Self::Broken(msg.clone())),
            }
        }
    }

    #[cfg(not(unix))]
    impl AsyncOpenFile {
        /// Creates an async file from a standard file.
        pub fn from_std_file(file: std::fs::File) -> Self {
            Self::File(File::from_std(file))
        }

        /// Creates an async pipe reader from a blocking pipe reader.
        pub fn from_pipe_reader(_reader: std::io::PipeReader) -> io::Result<Self> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "async pipes not supported on non-unix",
            ))
        }

        /// Creates an async pipe writer from a blocking pipe writer.
        pub fn from_pipe_writer(_writer: std::io::PipeWriter) -> io::Result<Self> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "async pipes not supported on non-unix",
            ))
        }

        /// Tries to clone the open file.
        #[cfg(not(target_family = "wasm"))]
        pub fn try_clone(&self) -> io::Result<Self> {
            match self {
                Self::Stdin(_) => Ok(Self::Stdin(stdin())),
                Self::Stdout(_) => Ok(Self::Stdout(stdout())),
                Self::Stderr(_) => Ok(Self::Stderr(stderr())),
                Self::File(f) => {
                    let handle = tokio::runtime::Handle::current();
                    tokio::task::block_in_place(|| handle.block_on(f.try_clone())).map(Self::File)
                }
                Self::PipeReader(_) | Self::PipeWriter(_) => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "cannot clone pipes",
                )),
                Self::Stream(s) => Ok(Self::Stream(s.clone_box())),
                Self::Broken(msg) => Ok(Self::Broken(msg.clone())),
            }
        }

        /// Tries to clone the open file.
        #[cfg(target_family = "wasm")]
        pub fn try_clone(&self) -> io::Result<Self> {
            match self {
                Self::Stdin(_) => Ok(Self::Stdin(stdin())),
                Self::Stdout(_) => Ok(Self::Stdout(stdout())),
                Self::Stderr(_) => Ok(Self::Stderr(stderr())),
                Self::File(f) => f.try_clone().map(Self::File),
                Self::PipeReader(_) | Self::PipeWriter(_) => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "cannot clone pipes",
                )),
                Self::Stream(s) => Ok(Self::Stream(s.clone_box())),
                Self::Broken(msg) => Ok(Self::Broken(msg.clone())),
            }
        }
    }

    impl AsyncOpenFile {
        /// Reads bytes asynchronously into the provided buffer.
        ///
        /// Returns the number of bytes read, or 0 on EOF.
        /// Reads bytes into the provided buffer.
        pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            use tokio::io::AsyncReadExt;
            match self {
                Self::Stdin(f) => f.read(buf).await,
                Self::Stdout(_) => Err(io::Error::other(error::ErrorKind::OpenFileNotReadable(
                    "stdout",
                ))),
                Self::Stderr(_) => Err(io::Error::other(error::ErrorKind::OpenFileNotReadable(
                    "stderr",
                ))),
                Self::File(f) => f.read(buf).await,
                Self::PipeReader(r) => r.read(buf).await,
                Self::PipeWriter(_) => Err(io::Error::other(
                    error::ErrorKind::OpenFileNotReadable("pipe writer"),
                )),
                Self::Stream(s) => Pin::new(s.as_mut()).read(buf).await,
                Self::Broken(msg) => Err(io::Error::other(msg.clone())),
            }
        }

        /// Reads all bytes until EOF into a new String.
        pub async fn read_to_string(&mut self) -> io::Result<String> {
            use tokio::io::AsyncReadExt;
            let mut s = String::new();
            match self {
                Self::Stdin(f) => f.read_to_string(&mut s).await?,
                Self::Stdout(_) => {
                    return Err(io::Error::other(error::ErrorKind::OpenFileNotReadable(
                        "stdout",
                    )));
                }
                Self::Stderr(_) => {
                    return Err(io::Error::other(error::ErrorKind::OpenFileNotReadable(
                        "stderr",
                    )));
                }
                Self::File(f) => f.read_to_string(&mut s).await?,
                Self::PipeReader(r) => r.read_to_string(&mut s).await?,
                Self::PipeWriter(_) => {
                    return Err(io::Error::other(error::ErrorKind::OpenFileNotReadable(
                        "pipe writer",
                    )));
                }
                Self::Stream(s_) => Pin::new(s_.as_mut()).read_to_string(&mut s).await?,
                Self::Broken(msg) => return Err(io::Error::other(msg.clone())),
            };
            Ok(s)
        }

        /// Writes bytes from the provided buffer.
        pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            use tokio::io::AsyncWriteExt;
            match self {
                Self::Stdin(_) => Err(io::Error::other(error::ErrorKind::OpenFileNotWritable(
                    "stdin",
                ))),
                Self::Stdout(f) => f.write(buf).await,
                Self::Stderr(f) => f.write(buf).await,
                Self::File(f) => f.write(buf).await,
                Self::PipeReader(_) => Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("pipe reader"),
                )),
                Self::PipeWriter(w) => w.write(buf).await,
                Self::Stream(s) => Pin::new(s.as_mut()).write(buf).await,
                Self::Broken(msg) => Err(io::Error::other(msg.clone())),
            }
        }

        /// Writes all bytes from the provided buffer.
        pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            use tokio::io::AsyncWriteExt;
            match self {
                Self::Stdin(_) => Err(io::Error::other(error::ErrorKind::OpenFileNotWritable(
                    "stdin",
                ))),
                Self::Stdout(f) => f.write_all(buf).await,
                Self::Stderr(f) => f.write_all(buf).await,
                Self::File(f) => f.write_all(buf).await,
                Self::PipeReader(_) => Err(io::Error::other(
                    error::ErrorKind::OpenFileNotWritable("pipe reader"),
                )),
                Self::PipeWriter(w) => w.write_all(buf).await,
                Self::Stream(s) => Pin::new(s.as_mut()).write_all(buf).await,
                Self::Broken(msg) => Err(io::Error::other(msg.clone())),
            }
        }

        /// Flushes the output stream.
        pub async fn flush(&mut self) -> io::Result<()> {
            use tokio::io::AsyncWriteExt;
            match self {
                Self::Stdin(_) => Ok(()),
                Self::Stdout(f) => f.flush().await,
                Self::Stderr(f) => f.flush().await,
                Self::File(f) => f.flush().await,
                Self::PipeReader(_) => Ok(()),
                Self::PipeWriter(w) => w.flush().await,
                Self::Stream(s) => Pin::new(s.as_mut()).flush().await,
                Self::Broken(msg) => Err(io::Error::other(msg.clone())),
            }
        }

        /// Checks if this file represents a terminal.
        pub fn is_terminal(&self) -> bool {
            match self {
                Self::Stdin(_) => std::io::stdin().is_terminal(),
                Self::Stdout(_) => std::io::stdout().is_terminal(),
                Self::Stderr(_) => std::io::stderr().is_terminal(),
                Self::File(_)
                | Self::PipeReader(_)
                | Self::PipeWriter(_)
                | Self::Stream(_)
                | Self::Broken(_) => false,
            }
        }

        /// Checks if this file represents a directory.
        pub async fn is_dir(&self) -> bool {
            match self {
                Self::File(f) => f.metadata().await.is_ok_and(|m| m.is_dir()),
                _ => false,
            }
        }

        /// Tries to convert the open file into an `OwnedFd`.
        #[cfg(unix)]
        pub fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, error::Error> {
            use std::os::fd::AsFd;
            match self {
                Self::Stdin(_) => Ok(std::io::stdin().as_fd().try_clone_to_owned()?),
                Self::Stdout(_) => Ok(std::io::stdout().as_fd().try_clone_to_owned()?),
                Self::Stderr(_) => Ok(std::io::stderr().as_fd().try_clone_to_owned()?),
                Self::File(f) => Ok(f.as_fd().try_clone_to_owned()?),
                Self::PipeReader(r) => {
                    let fd = r.as_fd().try_clone_to_owned()?;
                    set_fd_blocking(&fd)?;
                    Ok(fd)
                }
                Self::PipeWriter(w) => {
                    let fd = w.as_fd().try_clone_to_owned()?;
                    set_fd_blocking(&fd)?;
                    Ok(fd)
                }
                Self::Stream(s) => s.try_clone_to_owned(),
                Self::Broken(_) => Err(error::ErrorKind::CannotConvertToNativeFd.into()),
            }
        }

        /// Borrows the open file as a `BorrowedFd`.
        #[cfg(unix)]
        pub fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error> {
            use std::os::fd::AsFd;
            match self {
                Self::Stdin(f) => Ok(f.as_fd()),
                Self::Stdout(f) => Ok(f.as_fd()),
                Self::Stderr(f) => Ok(f.as_fd()),
                Self::File(f) => Ok(f.as_fd()),
                Self::PipeReader(r) => Ok(r.as_fd()),
                Self::PipeWriter(w) => Ok(w.as_fd()),
                Self::Stream(s) => s.try_borrow_as_fd(),
                Self::Broken(_) => Err(crate::error::ErrorKind::CannotConvertToNativeFd.into()),
            }
        }
    }

    impl Clone for AsyncOpenFile {
        fn clone(&self) -> Self {
            self.try_clone().unwrap_or_else(|err| {
                tracing::warn!("failed to clone open file ({err})");
                Self::Broken(err.to_string())
            })
        }
    }

    impl From<crate::ioutils::FailingReaderWriter> for AsyncOpenFile {
        fn from(frw: crate::ioutils::FailingReaderWriter) -> Self {
            Self::Broken(frw.message().to_owned())
        }
    }

    impl From<super::blocking::File> for AsyncOpenFile {
        fn from(file: super::blocking::File) -> Self {
            match file {
                super::blocking::File::Stdin(_) => Self::Stdin(stdin()),
                super::blocking::File::Stdout(_) => Self::Stdout(stdout()),
                super::blocking::File::Stderr(_) => Self::Stderr(stderr()),
                super::blocking::File::File(f) => Self::File(File::from_std(f)),
                super::blocking::File::PipeReader(r) => {
                    Self::from_pipe_reader(r).unwrap_or_else(|err| {
                        tracing::warn!("failed to convert pipe reader to async ({err})");
                        Self::Broken(err.to_string())
                    })
                }
                super::blocking::File::PipeWriter(w) => {
                    Self::from_pipe_writer(w).unwrap_or_else(|err| {
                        tracing::warn!("failed to convert pipe writer to async ({err})");
                        Self::Broken(err.to_string())
                    })
                }
                super::blocking::File::Stream(_) => {
                    Self::Broken("cannot convert blocking stream to async".to_owned())
                }
            }
        }
    }

    impl From<std::io::PipeReader> for AsyncOpenFile {
        fn from(reader: std::io::PipeReader) -> Self {
            Self::from_pipe_reader(reader).unwrap_or_else(|err| {
                tracing::warn!("failed to convert pipe reader to async ({err})");
                Self::Broken(err.to_string())
            })
        }
    }

    impl From<std::io::PipeWriter> for AsyncOpenFile {
        fn from(writer: std::io::PipeWriter) -> Self {
            Self::from_pipe_writer(writer).unwrap_or_else(|err| {
                tracing::warn!("failed to convert pipe writer to async ({err})");
                Self::Broken(err.to_string())
            })
        }
    }

    impl From<std::fs::File> for AsyncOpenFile {
        fn from(file: std::fs::File) -> Self {
            Self::File(File::from_std(file))
        }
    }

    impl From<std::io::Stdin> for AsyncOpenFile {
        fn from(_stdin: std::io::Stdin) -> Self {
            Self::Stdin(stdin())
        }
    }

    impl From<std::io::Stdout> for AsyncOpenFile {
        fn from(_stdout: std::io::Stdout) -> Self {
            Self::Stdout(stdout())
        }
    }

    impl From<std::io::Stderr> for AsyncOpenFile {
        fn from(_stderr: std::io::Stderr) -> Self {
            Self::Stderr(stderr())
        }
    }

    #[cfg(unix)]
    impl From<AsyncOpenFile> for std::process::Stdio {
        fn from(file: AsyncOpenFile) -> Self {
            match file {
                AsyncOpenFile::Stdin(_) => Self::inherit(),
                AsyncOpenFile::Stdout(_) => Self::inherit(),
                AsyncOpenFile::Stderr(_) => Self::inherit(),
                AsyncOpenFile::File(f) => f
                    .try_into_std()
                    .map_or_else(|_| Self::inherit(), Self::from),
                AsyncOpenFile::PipeReader(r) => r
                    .into_blocking_fd()
                    .map(std::fs::File::from)
                    .map_or_else(|_| Self::null(), Self::from),
                AsyncOpenFile::PipeWriter(w) => w
                    .into_blocking_fd()
                    .map(std::fs::File::from)
                    .map_or_else(|_| Self::null(), Self::from),
                AsyncOpenFile::Stream(_) | AsyncOpenFile::Broken(_) => Self::null(),
            }
        }
    }

    #[cfg(not(unix))]
    impl From<AsyncOpenFile> for std::process::Stdio {
        fn from(file: AsyncOpenFile) -> Self {
            match file {
                AsyncOpenFile::Stdin(_) => Self::inherit(),
                AsyncOpenFile::Stdout(_) => Self::inherit(),
                AsyncOpenFile::Stderr(_) => Self::inherit(),
                AsyncOpenFile::File(f) => f
                    .try_into_std()
                    .map_or_else(|_| Self::inherit(), Self::from),
                AsyncOpenFile::PipeReader(_) | AsyncOpenFile::PipeWriter(_) => Self::null(),
                AsyncOpenFile::Stream(_) | AsyncOpenFile::Broken(_) => Self::null(),
            }
        }
    }

    use std::collections::HashMap;

    use crate::ShellFd;

    /// Tristate representing an `AsyncOpenFile` entry in an `AsyncOpenFiles` structure.
    pub enum AsyncOpenFileEntry<'a> {
        /// File descriptor is present and has a valid associated `AsyncOpenFile`.
        Open(&'a AsyncOpenFile),
        /// File descriptor is explicitly marked as not being mapped to any `AsyncOpenFile`.
        NotPresent,
        /// File descriptor is not specified in any way; it may be provided by a
        /// parent context of some kind.
        NotSpecified,
    }

    /// Represents the open files in an async shell context.
    #[derive(Clone, Default)]
    pub struct AsyncOpenFiles {
        /// Maps shell file descriptors to async open files.
        files: HashMap<ShellFd, Option<AsyncOpenFile>>,
    }

    impl AsyncOpenFiles {
        /// File descriptor used for standard input.
        pub const STDIN_FD: ShellFd = 0;
        /// File descriptor used for standard output.
        pub const STDOUT_FD: ShellFd = 1;
        /// File descriptor used for standard error.
        pub const STDERR_FD: ShellFd = 2;

        /// First file descriptor available for non-stdio files.
        const FIRST_NON_STDIO_FD: ShellFd = 3;
        /// Maximum file descriptor number allowed.
        const MAX_FD: ShellFd = 1024;

        /// Creates a new `AsyncOpenFiles` instance populated with stdin, stdout, and stderr
        /// from the host environment.
        pub fn new() -> Self {
            Self {
                files: HashMap::from([
                    (Self::STDIN_FD, Some(AsyncOpenFile::Stdin(stdin()))),
                    (Self::STDOUT_FD, Some(AsyncOpenFile::Stdout(stdout()))),
                    (Self::STDERR_FD, Some(AsyncOpenFile::Stderr(stderr()))),
                ]),
            }
        }

        /// Retrieves the file backing standard input in this context.
        pub fn try_stdin(&self) -> Option<&AsyncOpenFile> {
            self.files.get(&Self::STDIN_FD).and_then(|f| f.as_ref())
        }

        /// Retrieves the file backing standard output in this context.
        pub fn try_stdout(&self) -> Option<&AsyncOpenFile> {
            self.files.get(&Self::STDOUT_FD).and_then(|f| f.as_ref())
        }

        /// Retrieves the file backing standard error in this context.
        pub fn try_stderr(&self) -> Option<&AsyncOpenFile> {
            self.files.get(&Self::STDERR_FD).and_then(|f| f.as_ref())
        }

        /// Tries to remove an async open file by its file descriptor.
        pub fn remove_fd(&mut self, fd: ShellFd) -> Option<AsyncOpenFile> {
            self.files.insert(fd, None).and_then(|f| f)
        }

        /// Tries to lookup the `AsyncOpenFile` associated with a file descriptor.
        pub fn try_fd(&self, fd: ShellFd) -> Option<&AsyncOpenFile> {
            self.files.get(&fd).and_then(|f| f.as_ref())
        }

        /// Tries to lookup the `AsyncOpenFile` associated with a file descriptor.
        pub fn fd_entry(&self, fd: ShellFd) -> AsyncOpenFileEntry<'_> {
            self.files.get(&fd).map_or(
                AsyncOpenFileEntry::NotSpecified,
                |opt_file| match opt_file {
                    Some(f) => AsyncOpenFileEntry::Open(f),
                    None => AsyncOpenFileEntry::NotPresent,
                },
            )
        }

        /// Checks if the given file descriptor is in use.
        pub fn contains_fd(&self, fd: ShellFd) -> bool {
            self.files.contains_key(&fd)
        }

        /// Associates the given file descriptor with the provided file.
        pub fn set_fd(&mut self, fd: ShellFd, file: AsyncOpenFile) -> Option<AsyncOpenFile> {
            self.files.insert(fd, Some(file)).and_then(|f| f)
        }

        /// Updates the open files from the provided iterator.
        pub fn update_from(&mut self, files: impl Iterator<Item = (ShellFd, AsyncOpenFile)>) {
            for (fd, file) in files {
                self.files.insert(fd, Some(file));
            }
        }

        /// Adds a new async open file, returning the assigned file descriptor.
        pub fn add(&mut self, file: AsyncOpenFile) -> Result<ShellFd, error::Error> {
            let mut fd = Self::FIRST_NON_STDIO_FD;
            while self.files.contains_key(&fd) {
                if fd >= Self::MAX_FD {
                    return Err(error::ErrorKind::TooManyOpenFiles.into());
                }
                fd += 1;
            }
            self.files.insert(fd, Some(file));
            Ok(fd)
        }

        /// Iterates over all file descriptors.
        pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, &AsyncOpenFile)> {
            self.files
                .iter()
                .filter_map(|(fd, file)| file.as_ref().map(|f| (*fd, f)))
        }
    }

    impl From<super::blocking::Files> for AsyncOpenFiles {
        fn from(open_files: super::blocking::Files) -> Self {
            Self {
                files: open_files
                    .files
                    .into_iter()
                    .map(|(fd, opt_file)| (fd, opt_file.map(AsyncOpenFile::from)))
                    .collect(),
            }
        }
    }

    impl<I> From<I> for AsyncOpenFiles
    where
        I: Iterator<Item = (ShellFd, AsyncOpenFile)>,
    {
        fn from(iter: I) -> Self {
            Self {
                files: iter.map(|(fd, file)| (fd, Some(file))).collect(),
            }
        }
    }
}

/// Re-export async types as the main public API.
pub use async_file::{
    AsyncOpenFile as OpenFile, AsyncOpenFileEntry as OpenFileEntry, AsyncOpenFiles as OpenFiles,
    AsyncStream as Stream,
};
