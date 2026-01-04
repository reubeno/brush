//! Library implementing interactive command input and completion for the brush shell.

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{InteractiveExecutionResult, InteractiveOptions, InteractiveShell};

mod input_backend;
pub use input_backend::{InputBackend, InteractivePrompt, ReadResult};

mod options;
pub use options::UIOptions;

mod refs;
pub use refs::ShellRef;

mod term_detection;
mod term_integration;
mod trace_categories;

#[cfg(feature = "highlighting")]
pub mod highlighting;

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
