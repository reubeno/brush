use std::{fmt::Display, sync::Arc};

/// Represents a position in source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourcePosition {
    /// The 0-based index of the character in the input stream.
    pub index: usize,
    /// The 1-based line number.
    pub line: usize,
    /// The 1-based column number.
    pub column: usize,
}

impl Display for SourcePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("line {} col {}", self.line, self.column))
    }
}

#[cfg(feature = "diagnostics")]
impl From<&SourcePosition> for miette::SourceOffset {
    #[allow(clippy::cast_sign_loss)]
    fn from(position: &SourcePosition) -> Self {
        position.index.into()
    }
}

/// Represents a span within source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourceSpan {
    /// The start position.
    pub start: Arc<SourcePosition>,
    /// The end position of the span (exclusive).
    pub end: Arc<SourcePosition>,
}

impl SourceSpan {
    /// Returns the length of the token in characters.
    pub fn length(&self) -> usize {
        self.end.index - self.start.index
    }
    pub(crate) fn within(start: &Self, end: &Self) -> Self {
        Self {
            start: start.start.clone(),
            end: end.end.clone(),
        }
    }
}
