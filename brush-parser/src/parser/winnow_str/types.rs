use winnow::stream::LocatingSlice;

use crate::parser::{ParserOptions, SourceInfo};

/// Type alias for input stream
pub type StrStream<'a> = LocatingSlice<&'a str>;

/// Context for parsing - holds options and source info
#[derive(Clone)]
pub struct ParseContext<'a> {
    /// Parser options controlling extended globbing, POSIX mode, etc.
    pub options: &'a ParserOptions,
    /// Source file information for error reporting
    pub source_info: &'a SourceInfo,
    /// Accumulated byte ranges of comments encountered at statement boundaries.
    /// Populated by the tracking whitespace parsers (`spaces_tracking`, `linebreak_tracking`,
    /// `newline_list_tracking`); converted to `SourceSpan`s and stored in `Program.comments`
    /// at the end of `parse_program`.
    pub comments: &'a std::cell::RefCell<Vec<std::ops::Range<usize>>>,
}
