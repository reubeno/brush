//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

#![deny(missing_docs)]

pub mod completion;

mod arithmetic;
pub mod builtins;
mod commands;
mod env;
mod error;
mod escape;
mod expansion;
mod extendedtests;
mod functions;
mod interp;
mod jobs;
mod keywords;
mod namedoptions;
mod openfiles;
mod options;
mod pathcache;
mod patterns;
mod processes;
mod prompt;
mod regex;
mod shell;
mod sys;
mod terminal;
mod tests;
mod timing;
mod trace_categories;
pub mod traps;
mod variables;

pub use commands::ExecutionContext;
pub use error::Error;
pub use interp::{ExecutionParameters, ExecutionResult};
pub use shell::{CreateOptions, Shell};
pub use terminal::TerminalControl;
pub use variables::{ShellValue, ShellVariable};
