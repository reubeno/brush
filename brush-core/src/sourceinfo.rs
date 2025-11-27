//! Source info.

use std::path::PathBuf;

/// Source context.
#[derive(Clone, Debug, Default)]
pub struct SourceInfo {
    /// The name of the source.
    pub source: String,
}

impl From<&str> for SourceInfo {
    fn from(source: &str) -> Self {
        Self {
            source: source.to_owned(),
        }
    }
}

impl From<PathBuf> for SourceInfo {
    fn from(path: PathBuf) -> Self {
        Self {
            source: path.to_string_lossy().to_string(),
        }
    }
}

impl std::fmt::Display for SourceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}
