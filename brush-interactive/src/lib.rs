//! Library implementing interactive command input and completion for the brush shell.

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{
    InputBackend, InteractiveExecutionResult, InteractivePrompt, InteractiveShellExt, ReadResult,
};

mod options;
pub use options::UIOptions;

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

mod refs;
mod trace_categories;

pub use refs::ShellRef;
