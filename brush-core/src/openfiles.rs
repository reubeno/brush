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
    Stdin,
    /// The original standard output this process was started with.
    Stdout,
    /// The original standard error this process was started with.
    Stderr,
    /// A null file that discards all input.
    Null,
    /// A file open for reading or writing.
    File(std::fs::File),
    /// A read end of a pipe.
    PipeReader(OpenPipeReader),
    /// A write end of a pipe.
    PipeWriter(OpenPipeWriter),
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
            Self::Stdin => Self::Stdin,
            Self::Stdout => Self::Stdout,
            Self::Stderr => Self::Stderr,
            Self::Null => Self::Null,
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
            Self::Stdin => Ok(std::io::stdin().as_fd().try_clone_to_owned()?),
            Self::Stdout => Ok(std::io::stdout().as_fd().try_clone_to_owned()?),
            Self::Stderr => Ok(std::io::stderr().as_fd().try_clone_to_owned()?),
            Self::Null => error::unimp("to_owned_fd for null open file"),
            Self::File(f) => Ok(f.into()),
            Self::PipeReader(r) => Ok(OwnedFd::from(r.0)),
            Self::PipeWriter(w) => Ok(OwnedFd::from(w.0)),
        }
    }

    /// Retrieves the raw file descriptor for the open file.
    #[cfg(unix)]
    #[expect(dead_code)]
    pub(crate) fn as_raw_fd(&self) -> Result<i32, error::Error> {
        match self {
            Self::Stdin => Ok(std::io::stdin().as_raw_fd()),
            Self::Stdout => Ok(std::io::stdout().as_raw_fd()),
            Self::Stderr => Ok(std::io::stderr().as_raw_fd()),
            Self::Null => error::unimp("as_raw_fd for null open file"),
            Self::File(f) => Ok(f.as_raw_fd()),
            Self::PipeReader(r) => Ok(r.0.as_raw_fd()),
            Self::PipeWriter(w) => Ok(w.0.as_raw_fd()),
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        match self {
            Self::Stdin | Self::Stdout | Self::Stderr | Self::Null => false,
            Self::File(file) => file.metadata().map(|m| m.is_dir()).unwrap_or(false),
            Self::PipeReader(_) | Self::PipeWriter(_) => false,
        }
    }

    pub(crate) fn is_term(&self) -> bool {
        match self {
            Self::Stdin => std::io::stdin().is_terminal(),
            Self::Stdout => std::io::stdout().is_terminal(),
            Self::Stderr => std::io::stderr().is_terminal(),
            Self::Null => false,
            Self::File(f) => f.is_terminal(),
            Self::PipeReader(_) => false,
            Self::PipeWriter(_) => false,
        }
    }

    pub(crate) fn get_term_attr(
        &self,
    ) -> Result<Option<sys::terminal::TerminalSettings>, error::Error> {
        if !self.is_term() {
            return Ok(None);
        }

        let result = match self {
            Self::Stdin => Some(sys::terminal::get_term_attr(std::io::stdin())?),
            Self::Stdout => Some(sys::terminal::get_term_attr(std::io::stdout())?),
            Self::Stderr => Some(sys::terminal::get_term_attr(std::io::stderr())?),
            Self::Null => None,
            Self::File(f) => Some(sys::terminal::get_term_attr(f)?),
            Self::PipeReader(_) => None,
            Self::PipeWriter(_) => None,
        };
        Ok(result)
    }

    pub(crate) fn set_term_attr(
        &self,
        termios: &sys::terminal::TerminalSettings,
    ) -> Result<(), error::Error> {
        match self {
            Self::Stdin => sys::terminal::set_term_attr_now(std::io::stdin(), termios)?,
            Self::Stdout => sys::terminal::set_term_attr_now(std::io::stdout(), termios)?,
            Self::Stderr => sys::terminal::set_term_attr_now(std::io::stderr(), termios)?,
            Self::Null => (),
            Self::File(f) => sys::terminal::set_term_attr_now(f, termios)?,
            Self::PipeReader(_) => (),
            Self::PipeWriter(_) => (),
        }
        Ok(())
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
            OpenFile::Stdin => Self::inherit(),
            OpenFile::Stdout => Self::inherit(),
            OpenFile::Stderr => Self::inherit(),
            OpenFile::Null => Self::null(),
            OpenFile::File(f) => f.into(),
            OpenFile::PipeReader(f) => f.0.into(),
            OpenFile::PipeWriter(f) => f.0.into(),
        }
    }
}

impl std::io::Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin => std::io::stdin().read(buf),
            Self::Stdout => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stdout",
            ))),
            Self::Stderr => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stderr",
            ))),
            Self::Null => Ok(0),
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
            Self::Stdin => Err(std::io::Error::other(error::Error::OpenFileNotWritable(
                "stdin",
            ))),
            Self::Stdout => std::io::stdout().write(buf),
            Self::Stderr => std::io::stderr().write(buf),
            Self::Null => Ok(buf.len()),
            Self::File(f) => f.write(buf),
            Self::PipeReader(_) => Err(std::io::Error::other(error::Error::OpenFileNotWritable(
                "pipe reader",
            ))),
            Self::PipeWriter(writer) => writer.0.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Stdin => Ok(()),
            Self::Stdout => std::io::stdout().flush(),
            Self::Stderr => std::io::stderr().flush(),
            Self::Null => Ok(()),
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
                (Self::STDIN_FD, OpenFile::Stdin),
                (Self::STDOUT_FD, OpenFile::Stdout),
                (Self::STDERR_FD, OpenFile::Stderr),
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
