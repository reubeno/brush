//! Implements a tokenizer and parsers for POSIX / bash shell syntax.

// #![feature(test)]
#![deny(missing_docs)]

pub mod arithmetic;
pub mod ast;
pub mod pattern;
pub mod prompt;
pub mod test_command;
pub mod word;

mod error;
mod parser;
mod parser2;
mod tokenizer;

pub use parser2::{parse_program, cacheable_parse_program};
pub use error::{ParseError, TestCommandParseError, WordParseError};
pub use parser::{parse_tokens, Parser, ParserOptions, SourceInfo};
pub use tokenizer::{tokenize_str, unquote_str, SourcePosition, Token, TokenLocation};
