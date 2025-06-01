//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

#![deny(missing_docs)]

pub mod completion;

mod arithmetic;
pub mod builtins;
mod commands;
pub mod env;
mod error;
mod escape;
mod expansion;
mod extendedtests;
pub mod functions;
pub mod interfaces;
mod interp;
mod jobs;
mod keywords;
mod namedoptions;
mod openfiles;
pub mod options;
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
pub mod variables;

pub use arithmetic::EvalError;
pub use brush_parser::ParseError;
pub use commands::{CommandArg, ExecutionContext};
pub use error::Error;
pub use interp::{ExecutionParameters, ExecutionResult, ProcessGroupPolicy};
pub use shell::{CreateOptions, Shell};
pub use terminal::TerminalControl;
pub use variables::{ShellValue, ShellVariable};
