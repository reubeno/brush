//! String-based winnow parser - mirrors winnow.rs but takes &str instead of &[Token]
//!
//! This module converts the token-based winnow parser to work directly on strings.
//! Each function is converted one at a time and tested against the original.

#![allow(dead_code)]

mod and_or;
mod arithmetic;
mod commands;
mod compound;
mod extended_test;
mod helpers;
mod pipelines;
mod position;
mod program;
mod redirections;
mod types;
mod words;

// Re-export public API
pub use position::PositionTracker;
pub use program::parse_program;
pub use types::{ParseContext, StrStream};
