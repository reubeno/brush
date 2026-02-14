use winnow::error::ContextError;
use winnow::stream::LocatingSlice;

use crate::parser::{ParserOptions, SourceInfo};

/// Type alias for parser error
pub(super) type PError = winnow::error::ErrMode<ContextError>;

/// Type alias for input stream
pub type StrStream<'a> = LocatingSlice<&'a str>;

/// Context for parsing - holds options and source info
#[derive(Clone)]
pub struct ParseContext<'a> {
    /// Parser options controlling extended globbing, POSIX mode, etc.
    pub options: &'a ParserOptions,
    /// Source file information for error reporting
    pub source_info: &'a SourceInfo,
    /// Pending trailing content from here-docs that needs to be parsed as pipeline continuation
    /// (e.g., "| grep hello" from "cat <<EOF | grep hello")
    pub pending_heredoc_trailing: &'a std::cell::RefCell<Option<&'a str>>,
}
