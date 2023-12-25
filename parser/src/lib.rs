#![deny(clippy::all)]
// #![deny(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::if_same_then_else)]

mod arithmetic;
pub mod ast;
mod parser;
pub mod prompt;
mod tokenizer;
pub mod word;

pub use parser::{parse_tokens, ParseResult, Parser, ParserOptions};
pub use tokenizer::tokenize_str;
pub use word::parse_word_for_expansion;
