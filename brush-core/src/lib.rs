//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

#![deny(missing_docs)]

pub mod completion;

mod arithmetic;
mod builtin;
mod builtins;
mod commands;
mod env;
mod error;
mod escape;
mod expansion;
mod extendedtests;
mod files;
mod functions;
mod interp;
mod jobs;
mod keywords;
mod namedoptions;
mod openfiles;
mod options;
mod patterns;
mod prompt;
mod regex;
mod shell;
mod tests;
mod traps;
mod users;
mod variables;

pub use error::Error;
pub use interp::ExecutionResult;
pub use shell::{CreateOptions, Shell};
