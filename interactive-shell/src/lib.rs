#![deny(clippy::all)]
// #![deny(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]

mod interactive_shell;

pub use interactive_shell::InteractiveShell;
