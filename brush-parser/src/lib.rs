//! Implements a tokenizer and parsers for POSIX / bash shell syntax.

pub mod arithmetic;
pub mod ast;
pub mod pattern;
pub mod prompt;
pub mod test_command;
pub mod word;

mod error;
mod parser;
mod tokenizer;

pub use error::{ParseError, TestCommandParseError, WordParseError};
pub use parser::{parse_tokens, Parser, ParserOptions, SourceInfo};
pub use tokenizer::{tokenize_str, Token, TokenLocation};
