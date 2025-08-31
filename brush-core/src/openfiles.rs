//! Managing files open within a shell instance.

use std::collections::HashMap;
use std::io::IsTerminal;
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::process::Stdio;

use crate::error;
use crate::sys;

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
    PipeReader(OpenPipeReader),
    /// A write end of a pipe.
    PipeWriter(OpenPipeWriter),
}

/// Returns an open file that will discard all I/O.
pub fn null() -> Result<OpenFile, error::Error> {
    let file = sys::fs::open_null_file()?;
    Ok(OpenFile::File(file))
}

impl Clone for OpenFile {
    fn clone(&self) -> Self {
        self.try_dup().unwrap()
    }
}

impl OpenFile {
    /// Tries to duplicate the open file.
    pub fn try_dup(&self) -> Result<Self, error::Error> {
        let result = match self {
            Self::Stdin(_) => Self::Stdin(std::io::stdin()),
            Self::Stdout(_) => Self::Stdout(std::io::stdout()),
            Self::Stderr(_) => Self::Stderr(std::io::stderr()),
            Self::File(f) => Self::File(f.try_clone()?),
            Self::PipeReader(f) => Self::PipeReader(f.0.try_clone()?.into()),
            Self::PipeWriter(f) => Self::PipeWriter(f.0.try_clone()?.into()),
        };

        Ok(result)
    }

    /// Converts the open file into an `OwnedFd`.
    #[cfg(unix)]
    pub(crate) fn into_owned_fd(self) -> Result<std::os::fd::OwnedFd, error::Error> {
        match self {
            Self::Stdin(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stdout(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::Stderr(f) => Ok(f.as_fd().try_clone_to_owned()?),
            Self::File(f) => Ok(f.into()),
            Self::PipeReader(r) => Ok(OwnedFd::from(r.0)),
            Self::PipeWriter(w) => Ok(OwnedFd::from(w.0)),
        }
    }

    /// Retrieves the raw file descriptor for the open file.
    #[cfg(unix)]
    #[expect(dead_code)]
    pub(crate) fn as_raw_fd(&self) -> i32 {
        match self {
            Self::Stdin(f) => f.as_raw_fd(),
            Self::Stdout(f) => f.as_raw_fd(),
            Self::Stderr(f) => f.as_raw_fd(),
            Self::File(f) => f.as_raw_fd(),
            Self::PipeReader(r) => r.0.as_raw_fd(),
            Self::PipeWriter(w) => w.0.as_raw_fd(),
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        match self {
            Self::Stdin(_) | Self::Stdout(_) | Self::Stderr(_) => false,
            Self::File(file) => file.metadata().map(|m| m.is_dir()).unwrap_or(false),
            Self::PipeReader(_) | Self::PipeWriter(_) => false,
        }
    }

    pub(crate) fn is_term(&self) -> bool {
        match self {
            Self::Stdin(f) => f.is_terminal(),
            Self::Stdout(f) => f.is_terminal(),
            Self::Stderr(f) => f.is_terminal(),
            Self::File(f) => f.is_terminal(),
            Self::PipeReader(_) => false,
            Self::PipeWriter(_) => false,
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for OpenFile {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        match self {
            Self::Stdin(f) => f.as_fd(),
            Self::Stdout(f) => f.as_fd(),
            Self::Stderr(f) => f.as_fd(),
            Self::File(f) => f.as_fd(),
            Self::PipeReader(r) => r.0.as_fd(),
            Self::PipeWriter(w) => w.0.as_fd(),
        }
    }
}

impl From<std::fs::File> for OpenFile {
    fn from(file: std::fs::File) -> Self {
        Self::File(file)
    }
}

impl From<OpenFile> for Stdio {
    fn from(open_file: OpenFile) -> Self {
        match open_file {
            OpenFile::Stdin(_) => Self::inherit(),
            OpenFile::Stdout(_) => Self::inherit(),
            OpenFile::Stderr(_) => Self::inherit(),
            OpenFile::File(f) => f.into(),
            OpenFile::PipeReader(f) => f.0.into(),
            OpenFile::PipeWriter(f) => f.0.into(),
        }
    }
}

impl std::io::Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(f) => f.read(buf),
            Self::Stdout(_) => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stdout",
            ))),
            Self::Stderr(_) => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stderr",
            ))),
            Self::File(f) => f.read(buf),
            Self::PipeReader(reader) => reader.0.read(buf),
            Self::PipeWriter(_) => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "pipe writer",
            ))),
        }
    }
}

impl std::io::Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(_) => Err(std::io::Error::other(error::Error::OpenFileNotWritable(
                "stdin",
            ))),
            Self::Stdout(f) => f.write(buf),
            Self::Stderr(f) => f.write(buf),
            Self::File(f) => f.write(buf),
            Self::PipeReader(_) => Err(std::io::Error::other(error::Error::OpenFileNotWritable(
                "pipe reader",
            ))),
            Self::PipeWriter(writer) => writer.0.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Stdin(_) => Ok(()),
            Self::Stdout(f) => f.flush(),
            Self::Stderr(f) => f.flush(),
            Self::File(f) => f.flush(),
            Self::PipeReader(_) => Ok(()),
            Self::PipeWriter(writer) => writer.0.flush(),
        }
    }
}

/// Represents the open files in a shell context.
#[derive(Clone)]
pub struct OpenFiles {
    /// Maps shell file descriptors to open files.
    files: HashMap<u32, OpenFile>,
}

impl Default for OpenFiles {
    fn default() -> Self {
        Self {
            files: HashMap::from([
                (Self::STDIN_FD, OpenFile::Stdin(std::io::stdin())),
                (Self::STDOUT_FD, OpenFile::Stdout(std::io::stdout())),
                (Self::STDERR_FD, OpenFile::Stderr(std::io::stderr())),
            ]),
        }
    }
}

impl OpenFiles {
    /// File descriptor used for standard input.
    pub const STDIN_FD: u32 = 0;
    /// File descriptor used for standard output.
    pub const STDOUT_FD: u32 = 1;
    /// File descriptor used for standard error.
    pub const STDERR_FD: u32 = 2;

    /// Tries to clone the open files.
    pub fn try_clone(&self) -> Result<Self, error::Error> {
        let mut files = HashMap::new();
        for (fd, file) in &self.files {
            files.insert(*fd, file.try_dup()?);
        }

        Ok(Self { files })
    }

    /// Retrieves the file backing standard input in this context.
    pub fn stdin(&self) -> Option<&OpenFile> {
        self.files.get(&0)
    }

    /// Retrieves the file backing standard output in this context.
    pub fn stdout(&self) -> Option<&OpenFile> {
        self.files.get(&1)
    }

    /// Retrieves the file backing standard error in this context.
    pub fn stderr(&self) -> Option<&OpenFile> {
        self.files.get(&2)
    }

    /// Tries to remove an open file by its file descriptor. If the file descriptor
    /// is not used, `None` will be returned; otherwise, the removed file will
    /// be returned.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to remove.
    pub fn remove(&mut self, fd: u32) -> Option<OpenFile> {
        self.files.remove(&fd)
    }

    /// Tries to lookup the `OpenFile` associated with a file descriptor. If the
    /// file descriptor is not used, `None` will be returned; otherwise, a reference
    /// to the `OpenFile` will be returned.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to lookup.
    pub fn get(&self, fd: u32) -> Option<&OpenFile> {
        self.files.get(&fd)
    }

    /// Checks if the given file descriptor is in use.
    pub fn contains(&self, fd: u32) -> bool {
        self.files.contains_key(&fd)
    }

    /// Checks if there are no open files in this context.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Associates the given file descriptor with the provided file. If the file descriptor
    /// is already in use, the previous file will be returned; otherwise, `None`
    /// will be returned.
    ///
    /// Arguments:
    ///
    /// * `fd`: The file descriptor to associate with the file.
    /// * `file`: The file to associate with the file descriptor.
    pub fn set(&mut self, fd: u32, file: OpenFile) -> Option<OpenFile> {
        self.files.insert(fd, file)
    }
}

impl IntoIterator for OpenFiles {
    type Item = (u32, OpenFile);
    type IntoIter = <std::collections::HashMap<u32, OpenFile> as std::iter::IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.files.into_iter()
    }
}

/// Creates a new pipe, returning its reader and writer ends.
pub fn pipe() -> Result<(OpenPipeReader, OpenPipeWriter), error::Error> {
    let (reader, writer) = sys::pipes::pipe()?;
    Ok((OpenPipeReader(reader), OpenPipeWriter(writer)))
}

/// An opaque wrapper around a pipe reader implementation.
pub struct OpenPipeReader(sys::pipes::PipeReader);

impl From<sys::pipes::PipeReader> for OpenPipeReader {
    fn from(reader: sys::pipes::PipeReader) -> Self {
        Self(reader)
    }
}

impl From<OpenPipeReader> for OpenFile {
    fn from(value: OpenPipeReader) -> Self {
        Self::PipeReader(value)
    }
}

impl From<OpenPipeReader> for sys::pipes::PipeReader {
    fn from(reader: OpenPipeReader) -> Self {
        reader.0
    }
}

/// An opaque wrapper around a pipe writer implementation.
pub struct OpenPipeWriter(sys::pipes::PipeWriter);

impl From<sys::pipes::PipeWriter> for OpenPipeWriter {
    fn from(writer: sys::pipes::PipeWriter) -> Self {
        Self(writer)
    }
}

impl From<OpenPipeWriter> for OpenFile {
    fn from(value: OpenPipeWriter) -> Self {
        Self::PipeWriter(value)
    }
}

impl From<OpenPipeWriter> for sys::pipes::PipeWriter {
    fn from(writer: OpenPipeWriter) -> Self {
        writer.0
    }
}
