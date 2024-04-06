use anyhow::Result;
use std::collections::HashMap;
use std::process::Stdio;

pub enum OpenFile {
    Stdout,
    Stderr,
    Null,
    File(std::fs::File),
    PipeReader(os_pipe::PipeReader),
    PipeWriter(os_pipe::PipeWriter),
    ProcessSubstitutionFile(std::fs::File),
    HereDocument(String),
}

impl OpenFile {
    pub fn try_dup(&self) -> Result<OpenFile> {
        let result = match self {
            OpenFile::Stdout => OpenFile::Stdout,
            OpenFile::Stderr => OpenFile::Stderr,
            OpenFile::Null => OpenFile::Null,
            OpenFile::File(f) => OpenFile::File(f.try_clone()?),
            OpenFile::PipeReader(f) => OpenFile::PipeReader(f.try_clone()?),
            OpenFile::PipeWriter(f) => OpenFile::PipeWriter(f.try_clone()?),
            OpenFile::ProcessSubstitutionFile(f) => {
                OpenFile::ProcessSubstitutionFile(f.try_clone()?)
            }
            OpenFile::HereDocument(doc) => OpenFile::HereDocument(doc.clone()),
        };

        Ok(result)
    }
}

impl From<OpenFile> for Stdio {
    fn from(open_file: OpenFile) -> Self {
        match open_file {
            OpenFile::Stdout => std::io::stdout().into(),
            OpenFile::Stderr => std::io::stderr().into(),
            OpenFile::Null => Stdio::null(),
            OpenFile::File(f) => f.into(),
            OpenFile::PipeReader(f) => f.into(),
            OpenFile::PipeWriter(f) => f.into(),
            OpenFile::ProcessSubstitutionFile(f) => f.into(),
            OpenFile::HereDocument(_) => Stdio::piped(),
        }
    }
}

impl std::io::Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            OpenFile::Stdout => std::io::stdout().write(buf),
            OpenFile::Stderr => std::io::stderr().write(buf),
            OpenFile::Null => Ok(buf.len()),
            OpenFile::File(f) => f.write(buf),
            OpenFile::PipeReader(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot write to pipe reader"
            ))),
            OpenFile::PipeWriter(writer) => writer.write(buf),
            OpenFile::ProcessSubstitutionFile(f) => f.write(buf),
            OpenFile::HereDocument(_) => Err(std::io::Error::other(anyhow::anyhow!(
                "cannot write to here document"
            ))),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            OpenFile::Stdout => std::io::stdout().flush(),
            OpenFile::Stderr => std::io::stderr().flush(),
            OpenFile::Null => Ok(()),
            OpenFile::File(f) => f.flush(),
            OpenFile::PipeReader(_) => Ok(()),
            OpenFile::PipeWriter(writer) => writer.flush(),
            OpenFile::ProcessSubstitutionFile(f) => f.flush(),
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
            // TODO: Figure out if we need to populate stdin here.
            files: HashMap::from([(1, OpenFile::Stdout), (2, OpenFile::Stderr)]),
        }
    }
}

impl OpenFiles {
    pub fn try_clone(&self) -> Result<OpenFiles> {
        let mut files = HashMap::new();
        for (fd, file) in &self.files {
            files.insert(*fd, file.try_dup()?);
        }

        Ok(OpenFiles { files })
    }
}
