//! Library implementing interactive command input and completion for the brush shell.

#![deny(missing_docs)]

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{InteractiveExecutionResult, InteractiveShell, ReadResult};

mod options;
pub use options::Options;

// Rustyline-based shell
#[cfg(feature = "rustyline")]
mod rustyline_shell;
#[cfg(feature = "rustyline")]
pub use rustyline_shell::RustylineShell;

// Basic shell
#[cfg(feature = "basic")]
mod basic_shell;
#[cfg(feature = "basic")]
pub use basic_shell::BasicShell;
