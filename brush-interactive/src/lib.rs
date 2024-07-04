//! Library implementing interactive command input and completion for the brush shell.

#![deny(missing_docs)]

#[cfg(any(unix, windows))]
mod interactive_shell;
#[cfg(any(unix, windows))]
pub use interactive_shell::{InteractiveShell, ShellError};

#[cfg(not(any(unix, windows)))]
mod basic_shell;
#[cfg(not(any(unix, windows)))]
pub use basic_shell::{InteractiveShell, ShellError};

mod options;
pub use options::Options;
