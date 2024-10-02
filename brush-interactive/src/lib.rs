//! Library implementing interactive command input and completion for the brush shell.

#![deny(missing_docs)]

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{
    InteractiveExecutionResult, InteractivePrompt, InteractiveShell, ReadResult,
};

mod options;
pub use options::Options;

#[cfg(any(windows, unix))]
mod completion;

// Reedline-based shell
#[cfg(feature = "reedline")]
mod reedline;
#[cfg(feature = "reedline")]
pub use reedline::ReedlineShell;

// Rustyline-based shell
#[cfg(feature = "rustyline")]
mod rustyline;
#[cfg(feature = "rustyline")]
pub use rustyline::RustylineShell;

// Basic shell
#[cfg(feature = "basic")]
mod basic;
#[cfg(feature = "basic")]
pub use basic::BasicShell;

mod trace_categories;
