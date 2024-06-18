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
    PipeReader(os_pipe::PipeReader),
    /// A write end of a pipe.
    PipeWriter(os_pipe::PipeWriter),
}

impl OpenFile {
    /// Tries to duplicate the open file.
    pub fn try_dup(&self) -> Result<OpenFile, error::Error> {
        let result = match self {
            OpenFile::Stdin => OpenFile::Stdin,
            OpenFile::Stdout => OpenFile::Stdout,
            OpenFile::Stderr => OpenFile::Stderr,
            OpenFile::Null => OpenFile::Null,
            OpenFile::File(f) => OpenFile::File(f.try_clone()?),
            OpenFile::PipeReader(f) => OpenFile::PipeReader(f.try_clone()?),
            OpenFile::PipeWriter(f) => OpenFile::PipeWriter(f.try_clone()?),
        };

        Ok(result)
    }

    /// Converts the open file into an `OwnedFd`.
    #[cfg(unix)]
    pub(crate) fn into_owned_fd(self) -> Result<std::os::fd::OwnedFd, error::Error> {
        match self {
            OpenFile::Stdin => Ok(std::io::stdin().as_fd().try_clone_to_owned()?),
            OpenFile::Stdout => Ok(std::io::stdout().as_fd().try_clone_to_owned()?),
            OpenFile::Stderr => Ok(std::io::stderr().as_fd().try_clone_to_owned()?),
            OpenFile::Null => error::unimp("to_owned_fd for null open file"),
            OpenFile::File(f) => Ok(f.into()),
            OpenFile::PipeReader(r) => Ok(OwnedFd::from(r)),
            OpenFile::PipeWriter(w) => Ok(OwnedFd::from(w)),
        }
    }

    /// Retrieves the raw file descriptor for the open file.
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn as_raw_fd(&self) -> Result<i32, error::Error> {
        match self {
            OpenFile::Stdin => Ok(std::io::stdin().as_raw_fd()),
            OpenFile::Stdout => Ok(std::io::stdout().as_raw_fd()),
            OpenFile::Stderr => Ok(std::io::stderr().as_raw_fd()),
            OpenFile::Null => error::unimp("as_raw_fd for null open file"),
            OpenFile::File(f) => Ok(f.as_raw_fd()),
            OpenFile::PipeReader(r) => Ok(r.as_raw_fd()),
            OpenFile::PipeWriter(w) => Ok(w.as_raw_fd()),
        }
    }

    pub(crate) fn is_term(&self) -> bool {
        match self {
            OpenFile::Stdin => std::io::stdin().is_terminal(),
            OpenFile::Stdout => std::io::stdout().is_terminal(),
            OpenFile::Stderr => std::io::stderr().is_terminal(),
            OpenFile::Null => false,
            OpenFile::File(f) => f.is_terminal(),
            OpenFile::PipeReader(_) => false,
            OpenFile::PipeWriter(_) => false,
        }
    }

    pub(crate) fn get_term_attr(&self) -> Result<Option<nix::sys::termios::Termios>, error::Error> {
        if !self.is_term() {
            return Ok(None);
        }

        let result = match self {
            OpenFile::Stdin => Some(nix::sys::termios::tcgetattr(std::io::stdin())?),
            OpenFile::Stdout => Some(nix::sys::termios::tcgetattr(std::io::stdout())?),
            OpenFile::Stderr => Some(nix::sys::termios::tcgetattr(std::io::stderr())?),
            OpenFile::Null => None,
            OpenFile::File(f) => Some(nix::sys::termios::tcgetattr(f)?),
            OpenFile::PipeReader(_) => None,
            OpenFile::PipeWriter(_) => None,
        };
        Ok(result)
    }

    pub(crate) fn set_term_attr(
        &self,
        action: nix::sys::termios::SetArg,
        termios: &nix::sys::termios::Termios,
    ) -> Result<(), error::Error> {
        match self {
            OpenFile::Stdin => nix::sys::termios::tcsetattr(std::io::stdin(), action, termios)?,
            OpenFile::Stdout => nix::sys::termios::tcsetattr(std::io::stdout(), action, termios)?,
            OpenFile::Stderr => nix::sys::termios::tcsetattr(std::io::stderr(), action, termios)?,
            OpenFile::Null => (),
            OpenFile::File(f) => nix::sys::termios::tcsetattr(f, action, termios)?,
            OpenFile::PipeReader(_) => (),
            OpenFile::PipeWriter(_) => (),
        }
        Ok(())
    }
}

impl From<OpenFile> for Stdio {
    fn from(open_file: OpenFile) -> Self {
        match open_file {
            OpenFile::Stdin => Stdio::inherit(),
            OpenFile::Stdout => Stdio::inherit(),
            OpenFile::Stderr => Stdio::inherit(),
            OpenFile::Null => Stdio::null(),
            OpenFile::File(f) => f.into(),
            OpenFile::PipeReader(f) => f.into(),
            OpenFile::PipeWriter(f) => f.into(),
        }
    }
}

impl std::io::Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            OpenFile::Stdin => std::io::stdin().read(buf),
            OpenFile::Stdout => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stdout",
            ))),
            OpenFile::Stderr => Err(std::io::Error::other(error::Error::OpenFileNotReadable(
                "stderr",
            ))),
            OpenFile::Null => Ok(0),
            OpenFile::File(f) => f.read(buf),
            OpenFile::PipeReader(reader) => reader.read(buf),
            OpenFile::PipeWriter(_) => Err(std::io::Error::other(
                error::Error::OpenFileNotReadable("pipe writer"),
            )),
        }
    }
}

impl std::io::Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            OpenFile::Stdin => Err(std::io::Error::other(error::Error::OpenFileNotWritable(
                "stdin",
            ))),
            OpenFile::Stdout => std::io::stdout().write(buf),
            OpenFile::Stderr => std::io::stderr().write(buf),
            OpenFile::Null => Ok(buf.len()),
            OpenFile::File(f) => f.write(buf),
            OpenFile::PipeReader(_) => Err(std::io::Error::other(
                error::Error::OpenFileNotWritable("pipe reader"),
            )),
            OpenFile::PipeWriter(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            OpenFile::Stdin => Ok(()),
            OpenFile::Stdout => std::io::stdout().flush(),
            OpenFile::Stderr => std::io::stderr().flush(),
            OpenFile::Null => Ok(()),
            OpenFile::File(f) => f.flush(),
            OpenFile::PipeReader(_) => Ok(()),
            OpenFile::PipeWriter(writer) => writer.flush(),
        }
    }
}

/// Represents the open files in a shell context.
pub struct OpenFiles {
    /// Maps shell file descriptors to open files.
    pub files: HashMap<u32, OpenFile>,
}

impl Clone for OpenFiles {
    fn clone(&self) -> Self {
        self.try_clone().unwrap()
    }
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

impl OpenFiles {
    /// Tries to clone the open files.
    pub fn try_clone(&self) -> Result<OpenFiles, error::Error> {
        let mut files = HashMap::new();
        for (fd, file) in &self.files {
            files.insert(*fd, file.try_dup()?);
        }

        Ok(OpenFiles { files })
    }
}
