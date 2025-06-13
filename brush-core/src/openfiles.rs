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
pub(crate) enum OpenFile {
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
    PipeReader(sys::pipes::PipeReader),
    /// A write end of a pipe.
    PipeWriter(sys::pipes::PipeWriter),
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
            Self::PipeReader(f) => Self::PipeReader(f.try_clone()?),
            Self::PipeWriter(f) => Self::PipeWriter(f.try_clone()?),
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
            Self::PipeReader(r) => Ok(OwnedFd::from(r)),
            Self::PipeWriter(w) => Ok(OwnedFd::from(w)),
        }
    }

    /// Retrieves the raw file descriptor for the open file.
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn as_raw_fd(&self) -> Result<i32, error::Error> {
        match self {
            Self::Stdin => Ok(std::io::stdin().as_raw_fd()),
            Self::Stdout => Ok(std::io::stdout().as_raw_fd()),
            Self::Stderr => Ok(std::io::stderr().as_raw_fd()),
            Self::Null => error::unimp("as_raw_fd for null open file"),
            Self::File(f) => Ok(f.as_raw_fd()),
            Self::PipeReader(r) => Ok(r.as_raw_fd()),
            Self::PipeWriter(w) => Ok(w.as_raw_fd()),
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
            OpenFile::PipeReader(f) => f.into(),
            OpenFile::PipeWriter(f) => f.into(),
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
            Self::PipeReader(reader) => reader.read(buf),
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
            Self::PipeWriter(writer) => writer.write(buf),
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
            Self::PipeWriter(writer) => writer.flush(),
        }
    }
}

/// Represents the open files in a shell context.
#[derive(Clone)]
pub struct OpenFiles {
    /// Maps shell file descriptors to open files.
    pub(crate) files: HashMap<u32, OpenFile>,
}

impl Default for OpenFiles {
    fn default() -> Self {
        Self {
            files: HashMap::from([
                (0, OpenFile::Stdin),
                (1, OpenFile::Stdout),
                (2, OpenFile::Stderr),
            ]),
        }
    }
}

#[allow(dead_code)]
impl OpenFiles {
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
}
