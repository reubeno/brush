//! Implements a tokenizer and parsers for POSIX / bash shell syntax.

#![deny(missing_docs)]

pub mod arithmetic;
pub mod ast;
pub mod pattern;
pub mod prompt;
pub mod readline_binding;
pub mod test_command;
pub mod word;

mod error;
mod parser;
mod tokenizer;

#[cfg(test)]
mod snapshot_tests;

pub use error::{BindingParseError, ParseError, TestCommandParseError, WordParseError};
pub use parser::{parse_tokens, Parser, ParserOptions, SourceInfo};
pub use tokenizer::{
    tokenize_str, tokenize_str_with_options, uncached_tokenize_str, unquote_str, SourcePosition,
    Token, TokenLocation, TokenizerError, TokenizerOptions,
};
