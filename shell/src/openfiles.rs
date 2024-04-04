use anyhow::Result;
use std::collections::HashMap;
use std::process::Stdio;

pub(crate) enum OpenFile {
    Stdout,
    Stderr,
    File(std::fs::File),
    ProcessSubstitutionFile(std::fs::File),
    HereDocument(String),
}

impl OpenFile {
    pub fn try_dup(&self) -> Result<OpenFile> {
        let result = match self {
            OpenFile::Stdout => OpenFile::Stdout,
            OpenFile::Stderr => OpenFile::Stderr,
            OpenFile::File(f) => OpenFile::File(f.try_clone()?),
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
            OpenFile::File(f) => f.into(),
            OpenFile::ProcessSubstitutionFile(f) => f.into(),
            OpenFile::HereDocument(_) => Stdio::piped(),
        }
    }
}

impl std::io::Write for &OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = match self {
            OpenFile::Stdout => {
                return std::io::stdout().write(buf);
            }
            OpenFile::Stderr => {
                return std::io::stderr().write(buf);
            }
            OpenFile::File(f) => f,
            OpenFile::ProcessSubstitutionFile(f) => f,
            OpenFile::HereDocument(_) => {
                return Err(std::io::Error::other(anyhow::anyhow!(
                    "cannot write to here document"
                )))
            }
        };

        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = match self {
            OpenFile::Stdout => {
                return std::io::stdout().flush();
            }
            OpenFile::Stderr => {
                return std::io::stderr().flush();
            }
            OpenFile::File(f) => f,
            OpenFile::ProcessSubstitutionFile(f) => f,
            OpenFile::HereDocument(_) => return Ok(()),
        };

        file.flush()
    }
}

pub(crate) struct OpenFiles {
    pub files: HashMap<u32, OpenFile>,
}

impl OpenFiles {
    pub fn new() -> OpenFiles {
        OpenFiles {
            // TODO: Figure out if we need to populate stdin here.
            files: HashMap::from([(1, OpenFile::Stdout), (2, OpenFile::Stderr)]),
        }
    }
}
