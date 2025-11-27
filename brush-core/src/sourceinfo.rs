//! Source info.

use std::{fmt::Display, path::PathBuf, sync::Arc};

/// Source context.
#[derive(Clone, Debug, Default)]
pub struct SourceInfo {
    /// The name of the source.
    pub source: String,
    /// Optionally indicates a starting location after the beginning of the source.
    /// If `None`, the start is the beginning of the source.
    pub start: Option<Arc<crate::SourcePosition>>,
}

impl From<&str> for SourceInfo {
    fn from(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            start: None,
        }
    }
}

impl From<PathBuf> for SourceInfo {
    fn from(path: PathBuf) -> Self {
        Self {
            source: path.to_string_lossy().to_string(),
            start: None,
        }
    }
}

impl std::fmt::Display for SourceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

/// Information about a source site.
#[derive(Clone, Debug, Default)]
pub struct SourceSite {
    /// Info regarding the containing source text.
    pub source_info: crate::SourceInfo,
    /// The relative location of the site within the source text, if available.
    pub position: Option<Arc<crate::SourcePosition>>,
}

impl Display for SourceSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source_info.source)?;
        if let Some(pos) = &self.position {
            write!(f, ":{},{}", pos.line, pos.column)?;
        }

        Ok(())
    }
}
