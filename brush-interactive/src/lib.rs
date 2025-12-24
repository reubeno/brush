//! Library implementing interactive command input and completion for the brush shell.

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{InteractiveExecutionResult, InteractiveShell};

mod input_backend;
pub use input_backend::{InputBackend, InteractivePrompt, ReadResult};

mod options;
pub use options::UIOptions;

mod refs;
pub use refs::ShellRef;

mod term;
pub use term::{KnownTerminal, TerminalInfo};

mod term_integration;
pub use term_integration::TerminalIntegration;

mod trace_categories;

#[cfg(feature = "completion")]
mod completion;

// Reedline-based shell
#[cfg(feature = "reedline")]
mod reedline;
#[cfg(feature = "reedline")]
pub use reedline::ReedlineInputBackend;

// Basic shell
#[cfg(feature = "basic")]
mod basic;
#[cfg(feature = "basic")]
pub use basic::BasicInputBackend;

// Minimal shell
#[cfg(feature = "minimal")]
mod minimal;
#[cfg(feature = "minimal")]
pub use minimal::MinimalInputBackend;
