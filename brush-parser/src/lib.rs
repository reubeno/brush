//! Implements a tokenizer and parsers for POSIX / bash shell syntax.
//!
//! See [`docs::migrations`] for guidance on migrating between crate versions.

// TODO(unwrap): remove or scope this allow attribute
#![allow(clippy::unwrap_used)]

pub mod arithmetic;
pub mod ast;
/// Guides for users of this crate.
pub mod docs {
    #[doc = include_str!("../migrations/0.5.0.md")]
    pub mod migrations {}
}
pub mod pattern;
pub mod prompt;
pub mod readline_binding;
pub mod test_command;
pub mod word;

mod error;
mod parser;
mod source;
mod tokenizer;

#[cfg(test)]
mod snapshot_tests;

pub use error::{
    BindingParseError, ParseError, ParseErrorLocation, TestCommandParseError, WordParseError,
};

#[cfg(feature = "diagnostics")]
pub use error::miette::PrettyError;

#[cfg(feature = "winnow-parser")]
pub use parser::winnow_str;
pub use parser::{Parser, ParserBuilder, ParserImpl, ParserOptions, SourceInfo, parse_tokens};

pub use source::{SourcePosition, SourcePositionOffset, SourceSpan};
pub use tokenizer::{
    Token, TokenLocation, TokenizerError, TokenizerOptions, tokenize_str,
    tokenize_str_with_options, uncached_tokenize_str, unquote_str,
};
