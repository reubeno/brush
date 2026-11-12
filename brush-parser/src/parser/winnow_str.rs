//! A shell parser written with winnow that reads the script's text directly,
//! with no tokenizing pass.

mod and_or;
mod arithmetic;
mod commands;
mod compound;
mod continuations;
mod extended_test;
mod helpers;
mod parse_state;
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
