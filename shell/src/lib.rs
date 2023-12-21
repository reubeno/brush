#![deny(clippy::all)]
// #![deny(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::if_same_then_else)]

mod builtin;
mod builtins;
mod context;
mod expansion;
mod interp;
mod namedoptions;
mod options;
mod patterns;
mod prompt;
mod shell;

pub use interp::ExecutionResult;
pub use shell::{Shell, ShellCreateOptions};
