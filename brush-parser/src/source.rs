use std::{fmt::Display, sync::Arc};

/// Represents a position in source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourcePosition {
    /// The 0-based byte offset in the input stream.
    pub offset: usize,
    /// The 1-based line number.
    pub line: usize,
    /// The 1-based column number (character count within the line).
    pub column: usize,
}

impl Display for SourcePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{},{}", self.line, self.column))
    }
}

impl SourcePosition {
    /// Returns a new `SourcePosition` offset by the given `SourcePositionOffset`.
    ///
    /// # Arguments
    ///
    /// * `offset` - The offset to apply.
    #[must_use]
    pub const fn offset(&self, offset: &SourcePositionOffset) -> Self {
        Self {
            offset: self.offset + offset.offset,
            line: self.line + offset.line,
            column: if offset.line == 0 {
                self.column + offset.column
            } else {
                offset.column + 1
            },
        }
    }
}

#[cfg(feature = "diagnostics")]
impl From<&SourcePosition> for miette::SourceOffset {
    #[allow(clippy::cast_sign_loss)]
    fn from(position: &SourcePosition) -> Self {
        position.offset.into()
    }
}

/// Represents an offset in source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourcePositionOffset {
    /// The 0-based byte offset.
    pub offset: usize,
    /// The 0-based line offset.
    pub line: usize,
    /// The 0-based column offset.
    pub column: usize,
}

/// Represents a span within source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
    /// Returns the length of the span in bytes.
    pub fn length(&self) -> usize {
        self.end.offset - self.start.offset
    }
    pub(crate) fn within(start: &Self, end: &Self) -> Self {
        Self {
            start: start.start.clone(),
            end: end.end.clone(),
        }
    }
}
