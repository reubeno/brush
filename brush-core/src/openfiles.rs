//! Managing files open within a shell instance.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;

use crate::ShellFd;
use crate::error;
use crate::sys;

/// A trait representing a stream that can be read from and written to.
/// This is used for custom stream implementations in `OpenFile`.
///
/// Types that implement this trait are expected to be cloneable via the
/// `clone_box` function.
pub trait Stream: std::io::Read + std::io::Write + Send + Sync {
    /// Clones the stream into a boxed trait object.
    fn clone_box(&self) -> Box<dyn Stream>;

    /// Converts the stream into an `OwnedFd`. Returns an error if the operation
    /// is not supported or if it fails.
    #[cfg(unix)]
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, error::Error>;

    /// Borrows the stream as a `BorrowedFd`. Returns an error if the operation
    /// is not supported or if it fails.
    #[cfg(unix)]
    fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error>;
}

/// Represents a file open in a shell context.
///
/// File- and pipe-backed variants hold their handle behind an [`Arc`], so cloning an `OpenFile`
/// shares the underlying descriptor by reference count. The shell opens a fresh context for each
/// subshell, command substitution, background job, and function call and runs them as in-process
/// tasks against one process-wide descriptor table (rather than via `fork(2)` like a traditional
/// shell); sharing the descriptor keeps deeply nested or highly concurrent execution from
/// exhausting that table. A descriptor is duplicated for real only when an independently owned
/// copy is needed to hand to an external child process — see [`OpenFile::try_clone_to_owned`] and
/// the `Stdio` conversion below.
pub enum OpenFile {
    /// The original standard input this process was started with.
    Stdin(std::io::Stdin),
    /// The original standard output this process was started with.
    Stdout(std::io::Stdout),
    /// The original standard error this process was started with.
    Stderr(std::io::Stderr),
    /// A file open for reading or writing.
    File(Arc<std::fs::File>),
    /// A read end of a pipe.
    PipeReader(Arc<std::io::PipeReader>),
    /// A write end of a pipe.
    PipeWriter(Arc<std::io::PipeWriter>),
    /// A custom stream.
    Stream(Box<dyn Stream>),
}

#[cfg(feature = "serde")]
impl serde::Serialize for OpenFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Stdin(_) => serializer.serialize_str("stdin"),
            Self::Stdout(_) => serializer.serialize_str("stdout"),
            Self::Stderr(_) => serializer.serialize_str("stderr"),
            Self::File(_) => serializer.serialize_str("file"),
            Self::PipeReader(_) => serializer.serialize_str("pipe_reader"),
            Self::PipeWriter(_) => serializer.serialize_str("pipe_writer"),
            Self::Stream(_) => serializer.serialize_str("stream"),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for OpenFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "stdin" => return Ok(std::io::stdin().into()),
            "stdout" => return Ok(std::io::stdout().into()),
            "stderr" => return Ok(std::io::stderr().into()),
            "file" => (),
            "pipe_reader" => (),
            "pipe_writer" => (),
            "stream" => (),
            _ => return Err(serde::de::Error::custom("invalid open file")),
        }

        // TODO(serde): Figure out something better to do with open pipes and files.
        null().map_err(serde::de::Error::custom)
    }
}

/// Returns an open file that will discard all I/O.
pub fn null() -> Result<OpenFile, error::Error> {
    let file = sys::fs::open_null_file()?;
    Ok(file.into())
}

impl Clone for OpenFile {
    fn clone(&self) -> Self {
        match self {
            Self::Stdin(_) => std::io::stdin().into(),
            Self::Stdout(_) => std::io::stdout().into(),
            Self::Stderr(_) => std::io::stderr().into(),
            // File and pipe handles are shared by reference count; cloning never issues a
            // syscall and so cannot fail.
            Self::File(f) => Self::File(Arc::clone(f)),
            Self::PipeReader(r) => Self::PipeReader(Arc::clone(r)),
            Self::PipeWriter(w) => Self::PipeWriter(Arc::clone(w)),
            Self::Stream(s) => Self::Stream(s.clone_box()),
        }
    }
}

impl std::fmt::Display for OpenFile {
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

impl OpenFile {
    /// Converts the open file into an `OwnedFd`. For shared file/pipe handles this materializes
    /// a real duplicate via `dup(2)` so the caller receives an independently owned descriptor.
    #[cfg(unix)]
    pub(crate) fn try_clone_to_owned(self) -> Result<std::os::fd::OwnedFd, error::Error> {
        use std::os::fd::AsFd as _;

        match self {
            Self::Stdin(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stdout(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stderr(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::File(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::PipeReader(r) => Ok(r.as_fd().try_clone_to_owned()?),
            Self::PipeWriter(w) => Ok(w.as_fd().try_clone_to_owned()?),
            Self::Stream(s) => s.try_clone_to_owned(),
        }
    }

    /// Borrows the open file as a `BorrowedFd`.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation is not supported for the underlying file type.
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

impl From<std::io::Stdin> for OpenFile {
    /// Creates an `OpenFile` from standard input.
    fn from(stdin: std::io::Stdin) -> Self {
        Self::Stdin(stdin)
    }
}

impl From<std::io::Stdout> for OpenFile {
    /// Creates an `OpenFile` from standard output.
    fn from(stdout: std::io::Stdout) -> Self {
        Self::Stdout(stdout)
    }
}

impl From<std::io::Stderr> for OpenFile {
    /// Creates an `OpenFile` from standard error.
    fn from(stderr: std::io::Stderr) -> Self {
        Self::Stderr(stderr)
    }
}

impl From<std::fs::File> for OpenFile {
    fn from(file: std::fs::File) -> Self {
        Self::File(Arc::new(file))
    }
}

impl From<std::io::PipeReader> for OpenFile {
    fn from(reader: std::io::PipeReader) -> Self {
        Self::PipeReader(Arc::new(reader))
    }
}

impl From<std::io::PipeWriter> for OpenFile {
    fn from(writer: std::io::PipeWriter) -> Self {
        Self::PipeWriter(Arc::new(writer))
    }
}

impl TryFrom<OpenFile> for Stdio {
    type Error = error::Error;

    fn try_from(open_file: OpenFile) -> Result<Self, Self::Error> {
        // File and pipe handles are shared behind an `Arc`, so the descriptor cannot be moved
        // out; duplicate it to give the child an independently owned descriptor. Duplication can
        // fail (e.g. under descriptor exhaustion), so the conversion is fallible and the error is
        // surfaced to the caller rather than silently degrading the child's streams.
        match open_file {
            OpenFile::Stdin(_) | OpenFile::Stdout(_) | OpenFile::Stderr(_) => Ok(Self::inherit()),
            OpenFile::File(f) => Ok(f.try_clone()?.into()),
            OpenFile::PipeReader(r) => Ok(r.try_clone()?.into()),
            OpenFile::PipeWriter(w) => Ok(w.try_clone()?.into()),
            // Custom streams have no descriptor to hand to a child process.
            OpenFile::Stream(_) => Ok(Self::null()),
        }
    }
}

impl std::io::Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(f) => f.read(buf),
            Self::Stdout(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotReadable("stdout"),
            )),
            Self::Stderr(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotReadable("stderr"),
            )),
            // The handle is shared behind an `Arc`; read through a shared reference (`&File`
            // and `&PipeReader` both implement `Read`).
            Self::File(f) => f.as_ref().read(buf),
            Self::PipeReader(reader) => reader.as_ref().read(buf),
            Self::PipeWriter(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotReadable("pipe writer"),
            )),
            Self::Stream(s) => s.read(buf),
        }
    }
}

impl std::io::Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotWritable("stdin"),
            )),
            Self::Stdout(f) => f.write(buf),
            Self::Stderr(f) => f.write(buf),
            // The handle is shared behind an `Arc`; write through a shared reference (`&File`
            // and `&PipeWriter` both implement `Write`).
            Self::File(f) => f.as_ref().write(buf),
            Self::PipeReader(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotWritable("pipe reader"),
            )),
            Self::PipeWriter(writer) => writer.as_ref().write(buf),
            Self::Stream(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Stdin(_) => Ok(()),
            Self::Stdout(f) => f.flush(),
            Self::Stderr(f) => f.flush(),
            Self::File(f) => f.as_ref().flush(),
            Self::PipeReader(_) => Ok(()),
            Self::PipeWriter(writer) => writer.as_ref().flush(),
            Self::Stream(s) => s.flush(),
        }
    }
}

/// Tristate representing the an `OpenFile` entry in an `OpenFiles` structure.
pub enum OpenFileEntry<'a> {
    /// File descriptor is present and has a valid associated `OpenFile`.
    Open(&'a OpenFile),
    /// File descriptor is explicitly marked as not being mapped to any `OpenFile`.
    NotPresent,
    /// File descriptor is not specified in any way; it may be provided by a
    /// parent context of some kind.
    NotSpecified,
}

/// Represents the open files in a shell context.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OpenFiles {
    /// Maps shell file descriptors to open files.
    files: HashMap<ShellFd, Option<OpenFile>>,
}

impl OpenFiles {
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

    /// Creates a new `OpenFiles` instance populated with stdin, stdout, and stderr
    /// from the host environment.
    pub(crate) fn new() -> Self {
        Self {
            files: HashMap::from([
                (Self::STDIN_FD, Some(std::io::stdin().into())),
                (Self::STDOUT_FD, Some(std::io::stdout().into())),
                (Self::STDERR_FD, Some(std::io::stderr().into())),
            ]),
        }
    }

    /// Updates the open files from the provided iterator of (fd number, `OpenFile`) pairs.
    /// Any existing entries for the provided file descriptors will be overwritten.
    ///
    /// # Arguments
    ///
    /// * `files`: An iterator of (fd number, `OpenFile`) pairs to update the open files with.
    pub fn update_from(&mut self, files: impl Iterator<Item = (ShellFd, OpenFile)>) {
        for (fd, file) in files {
            let _ = self.files.insert(fd, Some(file));
        }
    }

    /// Retrieves the file backing standard input in this context.
    pub fn try_stdin(&self) -> Option<&OpenFile> {
        self.files.get(&Self::STDIN_FD).and_then(|f| f.as_ref())
    }

    /// Retrieves the file backing standard output in this context.
    pub fn try_stdout(&self) -> Option<&OpenFile> {
        self.files.get(&Self::STDOUT_FD).and_then(|f| f.as_ref())
    }

    /// Retrieves the file backing standard error in this context.
    pub fn try_stderr(&self) -> Option<&OpenFile> {
        self.files.get(&Self::STDERR_FD).and_then(|f| f.as_ref())
    }

    /// Tries to remove an open file by its file descriptor. If the file descriptor
    /// is not used, `None` will be returned; otherwise, the removed file will
    /// be returned.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to remove.
    pub fn remove_fd(&mut self, fd: ShellFd) -> Option<OpenFile> {
        self.files.insert(fd, None).and_then(|f| f)
    }

    /// Tries to lookup the `OpenFile` associated with a file descriptor.
    /// Returns `None` if the file descriptor is not present.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to lookup.
    pub fn try_fd(&self, fd: ShellFd) -> Option<&OpenFile> {
        self.files.get(&fd).and_then(|f| f.as_ref())
    }

    /// Tries to lookup the `OpenFile` associated with a file descriptor. Returns
    /// an `OpenFileEntry` representing the state of the file descriptor.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to lookup.
    pub fn fd_entry(&self, fd: ShellFd) -> OpenFileEntry<'_> {
        self.files
            .get(&fd)
            .map_or(OpenFileEntry::NotSpecified, |opt_file| match opt_file {
                Some(f) => OpenFileEntry::Open(f),
                None => OpenFileEntry::NotPresent,
            })
    }

    /// Checks if the given file descriptor is in use.
    pub fn contains_fd(&self, fd: ShellFd) -> bool {
        self.files.contains_key(&fd)
    }

    /// Associates the given file descriptor with the provided file. If the file descriptor
    /// is already in use, the previous file will be returned; otherwise, `None`
    /// will be returned.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to associate with the file.
    /// * `file`: The file to associate with the file descriptor.
    pub fn set_fd(&mut self, fd: ShellFd, file: OpenFile) -> Option<OpenFile> {
        self.files.insert(fd, Some(file)).and_then(|f| f)
    }

    /// Iterates over all file descriptors.
    pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, &OpenFile)> {
        self.files
            .iter()
            .filter_map(|(fd, file)| file.as_ref().map(|f| (*fd, f)))
    }

    /// Adds a new open file, returning the assigned file descriptor.
    ///
    /// # Arguments
    ///
    /// * `file`: The open file to add.
    pub fn add(&mut self, file: OpenFile) -> Result<ShellFd, error::Error> {
        // Start searching for free file descriptors after the standard ones.
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
}

impl<I> From<I> for OpenFiles
where
    I: Iterator<Item = (ShellFd, OpenFile)>,
{
    fn from(iter: I) -> Self {
        let files = iter.map(|(fd, file)| (fd, Some(file))).collect();
        Self { files }
    }
}

/// Async file abstractions for non-blocking I/O operations.
pub mod async_file {
    use std::io::{self, IsTerminal, Read as _, Write as _};
    use std::pin::Pin;
    use std::sync::Arc;
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

    #[cfg(all(not(target_family = "wasm"), not(unix)))]
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

    /// Wraps a shared file handle so async I/O is performed synchronously through
    /// the `Arc`-backed descriptor.
    ///
    /// [`OpenFile`] holds file and pipe handles behind an `Arc` so descriptors are
    /// shared (by refcount) across the cloned shell contexts that every subshell,
    /// command substitution, and background job spawns. Duplicating the descriptor
    /// for every async access would defeat that sharing and re-introduce the fd
    /// exhaustion the sharing was added to prevent. Instead we read/write the
    /// shared descriptor in place via the `&File`/`&PipeReader`/`&PipeWriter`
    /// `Read`/`Write` impls, completing each async op synchronously.
    #[cfg(unix)]
    pub struct SharedFile(Arc<std::fs::File>);

    #[cfg(unix)]
    impl AsyncRead for SharedFile {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let n = self.get_mut().0.as_ref().read(buf.initialize_unfilled())?;
            buf.advance(n);
            Poll::Ready(Ok(()))
        }
    }

    #[cfg(unix)]
    impl AsyncWrite for SharedFile {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(self.get_mut().0.as_ref().write(buf))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(self.get_mut().0.as_ref().flush())
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.poll_flush(cx)
        }
    }

    /// Wraps a shared pipe reader (read end) for synchronous-through-`Arc` async reads.
    #[cfg(unix)]
    pub struct SharedPipeReader(Arc<std::io::PipeReader>);

    #[cfg(unix)]
    impl AsyncRead for SharedPipeReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let n = self.get_mut().0.as_ref().read(buf.initialize_unfilled())?;
            buf.advance(n);
            Poll::Ready(Ok(()))
        }
    }

    /// Wraps a shared pipe writer (write end) for synchronous-through-`Arc` async writes.
    #[cfg(unix)]
    pub struct SharedPipeWriter(Arc<std::io::PipeWriter>);

    #[cfg(unix)]
    impl AsyncWrite for SharedPipeWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(self.get_mut().0.as_ref().write(buf))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(self.get_mut().0.as_ref().flush())
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.poll_flush(cx)
        }
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
        File(SharedFile),
        /// The read end of a pipe.
        PipeReader(SharedPipeReader),
        /// The write end of a pipe.
        PipeWriter(SharedPipeWriter),
        /// A custom async stream.
        Stream(Box<dyn AsyncStream>),
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
            }
        }
    }

    #[cfg(unix)]
    impl AsyncOpenFile {
        /// Creates an async file from a standard file.
        pub fn from_std_file(file: std::fs::File) -> Self {
            Self::File(SharedFile(Arc::new(file)))
        }

        /// Creates an async pipe reader from a blocking pipe reader.
        pub fn from_pipe_reader(reader: std::io::PipeReader) -> io::Result<Self> {
            Ok(Self::PipeReader(SharedPipeReader(Arc::new(reader))))
        }

        /// Creates an async pipe writer from a blocking pipe writer.
        pub fn from_pipe_writer(writer: std::io::PipeWriter) -> io::Result<Self> {
            Ok(Self::PipeWriter(SharedPipeWriter(Arc::new(writer))))
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
            }
        }

        /// Checks if this file represents a terminal.
        pub fn is_terminal(&self) -> bool {
            match self {
                Self::Stdin(_) => std::io::stdin().is_terminal(),
                Self::Stdout(_) => std::io::stdout().is_terminal(),
                Self::Stderr(_) => std::io::stderr().is_terminal(),
                Self::File(_) | Self::PipeReader(_) | Self::PipeWriter(_) | Self::Stream(_) => {
                    false
                }
            }
        }
    }

    impl From<super::OpenFile> for AsyncOpenFile {
        fn from(file: super::OpenFile) -> Self {
            match file {
                super::OpenFile::Stdin(_) => Self::Stdin(stdin()),
                super::OpenFile::Stdout(_) => Self::Stdout(stdout()),
                super::OpenFile::Stderr(_) => Self::Stderr(stderr()),
                // Share the descriptor by refcount (move the `Arc`) rather than
                // duplicating it -- see `SharedFile`/`SharedPipeReader`/`SharedPipeWriter`.
                #[cfg(unix)]
                super::OpenFile::File(f) => Self::File(SharedFile(f)),
                #[cfg(unix)]
                super::OpenFile::PipeReader(r) => Self::PipeReader(SharedPipeReader(r)),
                #[cfg(unix)]
                super::OpenFile::PipeWriter(w) => Self::PipeWriter(SharedPipeWriter(w)),
                #[cfg(not(unix))]
                super::OpenFile::File(f) => f
                    .try_clone()
                    .ok()
                    .map_or_else(|| Self::Stdin(stdin()), |file| Self::File(File::from_std(file))),
                #[cfg(not(unix))]
                super::OpenFile::PipeReader(r) => {
                    r.try_clone()
                        .ok()
                        .and_then(|p| Self::from_pipe_reader(p).ok())
                        .unwrap_or_else(|| Self::Stdin(stdin()))
                }
                #[cfg(not(unix))]
                super::OpenFile::PipeWriter(w) => {
                    w.try_clone()
                        .ok()
                        .and_then(|p| Self::from_pipe_writer(p).ok())
                        .unwrap_or_else(|| Self::Stdout(stdout()))
                }
                super::OpenFile::Stream(_) => Self::Stdin(stdin()),
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
    #[derive(Default)]
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

    impl From<super::OpenFiles> for AsyncOpenFiles {
        fn from(open_files: super::OpenFiles) -> Self {
            Self {
                files: open_files
                    .files
                    .into_iter()
                    .map(|(fd, opt_file)| (fd, opt_file.map(AsyncOpenFile::from)))
                    .collect(),
            }
        }
    }
}
