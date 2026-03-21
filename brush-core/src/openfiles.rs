//! Managing files open within a shell instance.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::process::Stdio;

use crate::ShellFd;
use crate::error;
use crate::ioutils;
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
pub enum OpenFile {
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
    Ok(OpenFile::File(file))
}

impl Clone for OpenFile {
    fn clone(&self) -> Self {
        // If we fail to clone the open file for any reason, we return a special file
        // that discards all I/O. This allows us to avoid fatally erroring out.
        self.try_clone().unwrap_or_else(|_err| {
            ioutils::FailingReaderWriter::new("failed to duplicate open file").into()
        })
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
        Self::File(file)
    }
}

impl From<std::io::PipeReader> for OpenFile {
    fn from(reader: std::io::PipeReader) -> Self {
        Self::PipeReader(reader)
    }
}

impl From<std::io::PipeWriter> for OpenFile {
    fn from(writer: std::io::PipeWriter) -> Self {
        Self::PipeWriter(writer)
    }
}

impl From<OpenFile> for Stdio {
    fn from(open_file: OpenFile) -> Self {
        match open_file {
            OpenFile::Stdin(_) => Self::inherit(),
            OpenFile::Stdout(_) => Self::inherit(),
            OpenFile::Stderr(_) => Self::inherit(),
            OpenFile::File(f) => f.into(),
            OpenFile::PipeReader(f) => f.into(),
            OpenFile::PipeWriter(f) => f.into(),
            // NOTE: Custom streams cannot be converted to `Stdio`; we do our best here
            // and return a null device instead.
            OpenFile::Stream(_) => Self::null(),
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
            Self::File(f) => f.read(buf),
            Self::PipeReader(reader) => reader.read(buf),
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
    use std::io::{self, IsTerminal};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use crate::error;

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
        Stdin(tokio::io::Stdin),
        /// The original standard output.
        Stdout(tokio::io::Stdout),
        /// The original standard error.
        Stderr(tokio::io::Stderr),
        /// A file open for reading or writing.
        File(tokio::fs::File),
        /// The read end of a pipe.
        PipeReader(tokio::net::unix::pipe::Receiver),
        /// The write end of a pipe.
        PipeWriter(tokio::net::unix::pipe::Sender),
        /// A custom async stream.
        Stream(Box<dyn AsyncStream>),
    }

    /// Represents an async file open in a shell context.
    #[cfg(not(unix))]
    pub enum AsyncOpenFile {
        /// The original standard input.
        Stdin(tokio::io::Stdin),
        /// The original standard output.
        Stdout(tokio::io::Stdout),
        /// The original standard error.
        Stderr(tokio::io::Stderr),
        /// A file open for reading or writing.
        File(tokio::fs::File),
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
    }

    #[cfg(not(unix))]
    impl AsyncOpenFile {
        /// Creates an async file from a standard file.
        pub fn from_std_file(file: std::fs::File) -> Self {
            Self::File(tokio::fs::File::from_std(file))
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
                super::OpenFile::Stdin(_) => Self::Stdin(tokio::io::stdin()),
                super::OpenFile::Stdout(_) => Self::Stdout(tokio::io::stdout()),
                super::OpenFile::Stderr(_) => Self::Stderr(tokio::io::stderr()),
                super::OpenFile::File(f) => Self::File(tokio::fs::File::from_std(f)),
                super::OpenFile::PipeReader(r) => {
                    Self::from_pipe_reader(r).unwrap_or_else(|_| Self::Stdin(tokio::io::stdin()))
                }
                super::OpenFile::PipeWriter(w) => {
                    Self::from_pipe_writer(w).unwrap_or_else(|_| Self::Stdout(tokio::io::stdout()))
                }
                super::OpenFile::Stream(_) => Self::Stdin(tokio::io::stdin()),
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
                    (
                        Self::STDIN_FD,
                        Some(AsyncOpenFile::Stdin(tokio::io::stdin())),
                    ),
                    (
                        Self::STDOUT_FD,
                        Some(AsyncOpenFile::Stdout(tokio::io::stdout())),
                    ),
                    (
                        Self::STDERR_FD,
                        Some(AsyncOpenFile::Stderr(tokio::io::stderr())),
                    ),
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
