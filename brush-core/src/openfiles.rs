//! Managing files open within a shell instance.

use std::collections::HashMap;
use std::io::IsTerminal;
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
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, error::Error>;

    /// Borrows the stream as a `BorrowedFd`. Returns an error if the operation
    /// is not supported or if it fails.
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
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
/// copy is needed to hand to an external child process (see `OpenFile::try_clone_to_owned`, which
/// the native launcher uses for a process's standard streams and other descriptors alike).
#[non_exhaustive]
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
    /// A FIFO (a named pipe), opened by its path. Its other end may be one of the shell's
    /// own tasks, so reads and writes on it, like those on a pipe's end, don't hold up
    /// the runtime.
    Fifo(Arc<std::fs::File>),
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
            Self::Fifo(_) => serializer.serialize_str("fifo"),
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
            "fifo" => (),
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
            Self::Fifo(f) => Self::Fifo(Arc::clone(f)),
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
            Self::Fifo(_) => write!(f, "fifo"),
            Self::PipeReader(_) => write!(f, "pipe reader"),
            Self::PipeWriter(_) => write!(f, "pipe writer"),
            Self::Stream(_) => write!(f, "stream"),
        }
    }
}

impl OpenFile {
    /// Converts the open file into an `OwnedFd`. For shared file/pipe handles this materializes
    /// a real duplicate via `dup(2)` so the caller receives an independently owned descriptor.
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    pub(crate) fn try_clone_to_owned(self) -> Result<std::os::fd::OwnedFd, error::Error> {
        use std::os::fd::AsFd as _;

        match self {
            Self::Stdin(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stdout(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stderr(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::File(f) | Self::Fifo(f) => Ok(f.as_fd().try_clone_to_owned()?),
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
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    pub fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, error::Error> {
        use std::os::fd::AsFd as _;

        match self {
            Self::Stdin(f) => Ok(f.as_fd()),
            Self::Stdout(f) => Ok(f.as_fd()),
            Self::Stderr(f) => Ok(f.as_fd()),
            Self::File(f) | Self::Fifo(f) => Ok(f.as_fd()),
            Self::PipeReader(r) => Ok(r.as_fd()),
            Self::PipeWriter(w) => Ok(w.as_fd()),
            Self::Stream(s) => s.try_borrow_as_fd(),
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        match self {
            Self::Stdin(_) | Self::Stdout(_) | Self::Stderr(_) => false,
            Self::File(file) => file.metadata().is_ok_and(|m| m.is_dir()),
            Self::Fifo(_) | Self::PipeReader(_) | Self::PipeWriter(_) | Self::Stream(_) => false,
        }
    }

    /// Checks if the open file is associated with a terminal.
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Stdin(f) => f.is_terminal(),
            Self::Stdout(f) => f.is_terminal(),
            Self::Stderr(f) => f.is_terminal(),
            Self::File(f) => f.is_terminal(),
            Self::Fifo(_) | Self::PipeReader(_) | Self::PipeWriter(_) | Self::Stream(_) => false,
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
            // implements `Read`). The other end of a pipe may be one of the shell's own tasks,
            // so a read from one mustn't hold up the runtime.
            Self::File(f) => f.as_ref().read(buf),
            Self::Fifo(f) => sys::pipe_io::read_fifo(f, buf),
            Self::PipeReader(reader) => sys::pipe_io::read(reader, buf),
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
            // implements `Write`). The other end of a pipe may be one of the shell's own tasks,
            // so a write to one mustn't hold up the runtime.
            Self::File(f) => f.as_ref().write(buf),
            Self::Fifo(f) => sys::pipe_io::write_fifo(f, buf),
            Self::PipeReader(_) => Err(std::io::Error::other(
                error::ErrorKind::OpenFileNotWritable("pipe reader"),
            )),
            Self::PipeWriter(writer) => sys::pipe_io::write(writer, buf),
            Self::Stream(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Stdin(_) => Ok(()),
            Self::Stdout(f) => f.flush(),
            Self::Stderr(f) => f.flush(),
            Self::File(f) | Self::Fifo(f) => f.as_ref().flush(),
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

/// Asynchronous reads and writes on the shell's open files.
pub mod async_file {
    use std::borrow::Cow;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _, ReadBuf};

    use super::OpenFile;
    use crate::sys;

    /// An asynchronous view of an [`OpenFile`].
    ///
    /// A read from an empty pipe or a write to a full one waits for the pipe without
    /// blocking the thread: the task gives way, so the task at the other end, which may
    /// be one of the shell's own, gets to run even on a runtime with a single thread.
    /// Other files are read and written synchronously, as through the [`OpenFile`]
    /// itself; nothing is buffered, so what's written lands in the order it's written
    /// with respect to other writers, child processes included.
    ///
    /// The view shares the [`OpenFile`]'s handle rather than duplicating it.
    ///
    /// Besides [`AsyncRead`] and [`AsyncWrite`], it can be written to with
    /// [`write_async!`](crate::write_async) and [`writeln_async!`](crate::writeln_async),
    /// awaiting the result:
    ///
    /// ```ignore
    /// brush_core::writeln_async!(context.stdout_async(), "{name}={value}").await?;
    /// ```
    pub struct AsyncOpenFile {
        file: OpenFile,
        io: sys::pipe_io::AsyncIo,
    }

    impl From<OpenFile> for AsyncOpenFile {
        fn from(file: OpenFile) -> Self {
            Self {
                file,
                io: sys::pipe_io::AsyncIo::default(),
            }
        }
    }

    impl AsyncOpenFile {
        /// Returns the file this is a view of.
        pub const fn file(&self) -> &OpenFile {
            &self.file
        }

        /// Checks if the file is a terminal.
        pub fn is_terminal(&self) -> bool {
            self.file.is_terminal()
        }

        /// Writes `text`; await the result. The returned future owns the text, so it's
        /// `Send` whatever the text was made from; see [`write_async!`](crate::write_async).
        ///
        /// # Arguments
        ///
        /// * `text` - The text to write.
        pub fn write_text(
            &mut self,
            text: impl Into<Cow<'static, str>>,
        ) -> impl Future<Output = io::Result<()>> + Send + '_ {
            let text = text.into();
            async move { self.write_all(text.as_bytes()).await }
        }
    }

    impl AsyncRead for AsyncOpenFile {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let count = std::task::ready!(this.io.poll_read(
                cx,
                &mut this.file,
                buf.initialize_unfilled()
            ))?;
            buf.advance(count);
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for AsyncOpenFile {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            this.io.poll_write(cx, &mut this.file, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(io::Write::flush(&mut self.get_mut().file))
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.poll_flush(cx)
        }
    }

    #[cfg(test)]
    #[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
    mod tests {
        use std::io::{Read as _, Seek as _};
        use std::sync::Arc;

        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        use super::super::OpenFile;
        use super::AsyncOpenFile;

        fn assert_send<T: Send>(t: T) -> T {
            t
        }

        /// A write to a full pipe and the read that drains it, on a runtime with one
        /// thread and no other to hand tasks to: neither may block that thread. (Only
        /// unix waits for pipes; elsewhere they're read and written synchronously.)
        #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
        #[test]
        fn a_full_pipe_lets_its_reader_run_on_one_thread() -> std::io::Result<()> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let (reader, writer) = std::io::pipe()?;
                let mut reader = AsyncOpenFile::from(OpenFile::from(reader));
                let mut writer = AsyncOpenFile::from(OpenFile::from(writer));
                let data = vec![b'x'; 4 << 20];
                let expected = data.len();
                let writer_task = tokio::spawn(async move { writer.write_all(&data).await });
                let mut all = Vec::new();
                reader.read_to_end(&mut all).await?;
                writer_task.await.map_err(std::io::Error::other)??;
                assert_eq!(all.len(), expected);
                std::io::Result::Ok(())
            })
        }

        #[tokio::test]
        async fn write_async_and_writeln_async_work_on_a_view() -> std::io::Result<()> {
            let (reader, writer) = std::io::pipe()?;
            let mut writer = AsyncOpenFile::from(OpenFile::from(writer));
            let mut reader = AsyncOpenFile::from(OpenFile::from(reader));

            let name = "x";
            assert_send(crate::write_async!(writer, "{name}=")).await?;
            crate::writeln_async!(writer, "1").await?;
            crate::writeln_async!(writer, "plain").await?;
            crate::writeln_async!(writer).await?;
            drop(writer);

            let mut text = String::new();
            reader.read_to_string(&mut text).await?;
            assert_eq!(text, "x=1\nplain\n\n");
            Ok(())
        }

        /// A view writes through the descriptor its `OpenFile` holds rather than a
        /// duplicate, so the output lands where the shell's other users of it see it.
        #[tokio::test]
        async fn a_file_view_shares_its_descriptor() -> std::io::Result<()> {
            let file = OpenFile::from(tempfile::tempfile()?);
            let OpenFile::File(shared) = &file else {
                unreachable!("a file makes a file");
            };
            let shared = Arc::clone(shared);

            let mut view = AsyncOpenFile::from(file);
            assert_eq!(Arc::strong_count(&shared), 2);
            view.write_all(b"shared").await?;
            assert!(!view.is_terminal());
            drop(view);

            let mut file = Arc::into_inner(shared)
                .ok_or_else(|| std::io::Error::other("the view should have released its handle"))?;
            file.rewind()?;
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            assert_eq!(text, "shared");
            Ok(())
        }

        #[tokio::test]
        async fn the_wrong_direction_is_an_error() -> std::io::Result<()> {
            let (reader, writer) = std::io::pipe()?;
            let mut writer = AsyncOpenFile::from(OpenFile::from(writer));
            let mut reader = AsyncOpenFile::from(OpenFile::from(reader));

            let mut buf = [0u8; 1];
            assert!(writer.read(&mut buf).await.is_err());
            assert!(reader.write(b"x").await.is_err());
            Ok(())
        }

        /// A write without a reader fails rather than waiting for one.
        #[tokio::test]
        async fn a_write_without_a_reader_fails() -> std::io::Result<()> {
            let (reader, writer) = std::io::pipe()?;
            drop(reader);
            let mut writer = AsyncOpenFile::from(OpenFile::from(writer));
            let err = writer.write_all(&vec![b'x'; 4 << 20]).await.err();
            assert_eq!(err.map(|e| e.kind()), Some(std::io::ErrorKind::BrokenPipe));
            Ok(())
        }
    }
}
