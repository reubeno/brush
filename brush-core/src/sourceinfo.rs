use std::{borrow::Cow, path::PathBuf};

/// Information about the source of tokens.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SourceInfo {
    /// The name of the source containing the input text. May be a file path or a
    /// logical name of a source (e.g., "command line").
    pub source: SourceOrigin,
    /// The offset into the file at which the source text begins.
    pub start_offset: Option<crate::SourcePosition>,
}

impl From<PathBuf> for SourceInfo {
    fn from(value: PathBuf) -> Self {
        Self {
            source: SourceOrigin::File(value),
            ..Default::default()
        }
    }
}

impl From<SourceOrigin> for SourceInfo {
    fn from(value: SourceOrigin) -> Self {
        Self {
            source: value,
            ..Default::default()
        }
    }
}

impl SourceInfo {
    /// Extracts a string reference from the source info.
    pub fn to_cow_str(&self) -> Cow<'_, str> {
        self.source.to_cow_str()
    }
}

/// Identifies the origin of source text.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum SourceOrigin {
    /// The source's origin is unknown.
    #[default]
    Unknown,
    /// The source originated from the specified file.
    File(PathBuf),
    /// The source originated from interactive execution.
    Interactive,
    /// The source originated from a trap handler.
    TrapHandler {
        /// The signal to which the handler is attached.
        signal: String,
    },
    /// The source originated from a command substitution.
    CommandSubstitution,
    /// The source originated from a prompt command.
    PromptCommand,
    /// The source originated from a command string provided on the command line.
    CommandString,
}

impl SourceOrigin {
    /// Extracts a string reference from the source info.
    pub fn to_cow_str(&self) -> Cow<'_, str> {
        match self {
            Self::Unknown => "<unknown>".into(),
            Self::File(path) => path.to_string_lossy(),
            Self::Interactive => "(interactive)".into(),
            Self::TrapHandler { signal } => std::format!("({signal} trap)").into(),
            Self::CommandSubstitution => "(command substitution)".into(),
            Self::PromptCommand => "(prompt command)".into(),
            Self::CommandString => "-c".into(),
        }
    }

    /// Returns a bash-compatible name for the source, used for `BASH_SOURCE` array.
    /// Bash uses "main" for interactive input instead of a descriptive string.
    pub fn compat_name(&self) -> Cow<'_, str> {
        match self {
            Self::Interactive => "main".into(),
            _ => self.to_cow_str(),
        }
    }
}

impl std::fmt::Display for SourceOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "<unknown>"),
            Self::File(path) => write!(f, "{}", path.display()),
            Self::Interactive => write!(f, "(interactive)"),
            Self::TrapHandler { signal } => write!(f, "({signal} trap)"),
            Self::CommandSubstitution => write!(f, "(command substitution)"),
            Self::PromptCommand => write!(f, "(prompt command)"),
            Self::CommandString => write!(f, "-c"),
        }
    }
}

impl std::fmt::Display for SourceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)?;

        if let Some(start_offset) = &self.start_offset {
            write!(f, ":{},{}", start_offset.line, start_offset.column)?;
        }

        Ok(())
    }
}
