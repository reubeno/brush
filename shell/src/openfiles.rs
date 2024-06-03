use std::collections::HashMap;
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::process::Stdio;

use crate::error;

pub enum OpenFile {
    Stdin,
    Stdout,
    Stderr,
    Null,
    File(std::fs::File),
    PipeReader(os_pipe::PipeReader),
    PipeWriter(os_pipe::PipeWriter),
    HereDocument(String),
}

impl OpenFile {
    pub fn try_dup(&self) -> Result<OpenFile, error::Error> {
        let result = match self {
            OpenFile::Stdin => OpenFile::Stdin,
            OpenFile::Stdout => OpenFile::Stdout,
            OpenFile::Stderr => OpenFile::Stderr,
            OpenFile::Null => OpenFile::Null,
            OpenFile::File(f) => OpenFile::File(f.try_clone()?),
            OpenFile::PipeReader(f) => OpenFile::PipeReader(f.try_clone()?),
            OpenFile::PipeWriter(f) => OpenFile::PipeWriter(f.try_clone()?),
            OpenFile::HereDocument(doc) => OpenFile::HereDocument(doc.clone()),
        };

        Ok(result)
    }

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
            OpenFile::HereDocument(_) => error::unimp("to_owned_fd for here doc"),
        }
    }

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
            OpenFile::HereDocument(_) => error::unimp("as_raw_fd for here doc"),
        }
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
            OpenFile::HereDocument(_) => Stdio::piped(),
        }
    }
}

impl std::io::Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            OpenFile::Stdin => std::io::stdin().read(buf),
            OpenFile::Stdout => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot read from stdout"
            ))),
            OpenFile::Stderr => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot read from stderr"
            ))),
            OpenFile::Null => Ok(0),
            OpenFile::File(f) => f.read(buf),
            OpenFile::PipeReader(reader) => reader.read(buf),
            OpenFile::PipeWriter(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot read from pipe writer"
            ))),
            OpenFile::HereDocument(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot read from here document"
            ))),
        }
    }
}

impl std::io::Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            OpenFile::Stdin => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot write to stdin"
            ))),
            OpenFile::Stdout => std::io::stdout().write(buf),
            OpenFile::Stderr => std::io::stderr().write(buf),
            OpenFile::Null => Ok(buf.len()),
            OpenFile::File(f) => f.write(buf),
            OpenFile::PipeReader(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot write to pipe reader"
            ))),
            OpenFile::PipeWriter(writer) => writer.write(buf),
            OpenFile::HereDocument(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot write to here document"
            ))),
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
            OpenFile::HereDocument(_) => Ok(()),
        }
    }
}

pub struct OpenFiles {
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
    pub fn try_clone(&self) -> Result<OpenFiles, error::Error> {
        let mut files = HashMap::new();
        for (fd, file) in &self.files {
            files.insert(*fd, file.try_dup()?);
        }

        Ok(OpenFiles { files })
    }
}
