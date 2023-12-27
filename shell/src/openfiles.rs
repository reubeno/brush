use anyhow::Result;
use std::collections::HashMap;
use std::process::Stdio;

pub(crate) enum OpenFile {
    File(std::fs::File),
    ProcessSubstitutionFile(std::fs::File),
    HereDocument(String),
}

impl OpenFile {
    pub fn try_dup(&self) -> Result<OpenFile> {
        let result = match self {
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
            OpenFile::File(f) => f.into(),
            OpenFile::ProcessSubstitutionFile(f) => f.into(),
            OpenFile::HereDocument(_) => Stdio::piped(),
        }
    }
}

pub(crate) struct OpenFiles {
    pub files: HashMap<u32, OpenFile>,
}

impl OpenFiles {
    pub fn new() -> OpenFiles {
        OpenFiles {
            files: HashMap::new(),
        }
    }
}
