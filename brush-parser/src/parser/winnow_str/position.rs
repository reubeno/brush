use winnow::stream::LocatingSlice;

use crate::source::{SourcePosition, SourceSpan};

/// Maps offsets in the parsed text to lines and columns of the input.
///
/// The offsets of the input's newlines are found once, and a line is found from them by
/// binary search.
#[derive(Debug, Clone)]
pub struct PositionTracker {
    /// Offsets of the input's newlines.
    line_breaks: Vec<usize>,
    /// Length of the parsed text, which the rest of the stream is measured against.
    original_len: usize,
    /// Where the parsed text and the input part, as `(offset in the text, offset in the
    /// input)`, when the text is the input as bash reads it (see [`super::continuations`]).
    anchors: Vec<(usize, usize)>,
}

impl PositionTracker {
    /// Creates a tracker for `input`, parsed as it is.
    pub fn new(input: &str) -> Self {
        let line_breaks: Vec<usize> = input
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None })
            .collect();

        Self {
            line_breaks,
            original_len: input.len(),
            anchors: Vec::new(),
        }
    }

    /// A tracker for `parsed`, the input as bash reads it, mapped back to `input` through
    /// `anchors`: positions are reported in `input`.
    pub(super) fn with_anchors(input: &str, parsed: &str, anchors: Vec<(usize, usize)>) -> Self {
        Self {
            original_len: parsed.len(),
            anchors,
            ..Self::new(input)
        }
    }

    /// The offset in the input of `offset` in the parsed text.
    pub(super) fn input_offset(&self, offset: usize) -> usize {
        match self.anchors.partition_point(|&(at, _)| at <= offset) {
            0 => offset,
            n => {
                let (text_at, input_at) = self.anchors[n - 1];
                input_at + (offset - text_at)
            }
        }
    }

    /// Get current offset from `LocatingSlice`
    #[inline]
    pub(super) fn offset_from_locating(&self, input: &LocatingSlice<&str>) -> usize {
        self.original_len - input.len()
    }

    /// The source position of `offset` in the parsed text.
    fn position_at(&self, offset: usize) -> SourcePosition {
        let offset = self.input_offset(offset);
        // Binary search to find which line this offset is on
        // line_breaks[i] is the position of the i-th newline
        // Line numbering: line 1 is before first newline, line 2 is before second newline, etc.
        let line = match self.line_breaks.binary_search(&offset) {
            // Found exact newline character - it belongs to the line it ends
            Ok(pos) => pos + 1,
            // Not found - pos is where it would be inserted, so pos is the line number
            Err(pos) => pos + 1,
        };

        // Calculate column as offset from start of line
        let line_start = if line > 1 {
            // Previous line ended at line_breaks[line-2], so this line starts after that
            self.line_breaks[line - 2] + 1
        } else {
            // Line 1 starts at position 0
            0
        };

        SourcePosition {
            index: offset,
            line,
            column: offset.saturating_sub(line_start) + 1,
        }
    }

    /// The source span of a byte range of the parsed text.
    #[inline]
    pub(super) fn range_to_span(&self, range: std::ops::Range<usize>) -> SourceSpan {
        SourceSpan {
            start: self.position_at(range.start).into(),
            end: self.position_at(range.end).into(),
        }
    }
}
