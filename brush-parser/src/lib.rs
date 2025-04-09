//! Implements a tokenizer and parsers for POSIX / bash shell syntax.

#![deny(missing_docs)]

pub mod arithmetic;
pub mod ast;
pub mod pattern;
pub mod prompt;
pub mod test_command;
pub mod word;

mod error;
mod parser;
mod tokenizer;
mod winnow;

pub use error::{ParseError, TestCommandParseError, WordParseError};
pub use parser::{parse_tokens, Parser, ParserOptions, SourceInfo};
pub use tokenizer::{
    tokenize_str, tokenize_str_with_options, uncached_tokenize_str, unquote_str, SourcePosition,
    Token, TokenLocation, TokenizerOptions,
};
pub use winnow::WinnowParser;
