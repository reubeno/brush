//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

pub mod completion;

pub mod arithmetic;
mod braceexpansion;
pub mod builtins;
pub mod commands;
pub mod env;
pub mod error;
pub mod escape;
pub mod expansion;
mod extendedtests;
pub mod functions;
pub mod history;
pub mod interfaces;
mod interp;
pub mod jobs;
mod keywords;
pub mod namedoptions;
pub mod openfiles;
pub mod options;
pub mod pathcache;
pub mod pathsearch;
pub mod patterns;
pub mod processes;
mod prompt;
mod regex;
pub mod scripts;
mod shell;
pub mod sys;
mod terminal;
pub mod tests;
pub mod timing;
pub mod trace_categories;
pub mod traps;
pub mod variables;
mod wellknownvars;

pub use commands::{CommandArg, ExecutionContext};
pub use error::Error;
pub use interp::{ExecutionParameters, ExecutionResult, ProcessGroupPolicy};
pub use shell::{CreateOptions, Shell, ShellBuilder, ShellBuilderState};
pub use terminal::TerminalControl;
pub use variables::{ShellValue, ShellVariable};
