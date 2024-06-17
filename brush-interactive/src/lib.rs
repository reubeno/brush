//! Library implementing interactive command input and completion for the brush shell.

#![deny(missing_docs)]

mod interactive_shell;

pub use interactive_shell::{InteractiveShell, Options, ShellError};
