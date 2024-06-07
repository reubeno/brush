//! Core implementation of the brush shell

mod arithmetic;
mod builtin;
mod builtins;
mod commands;
mod completion;
mod context;
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

pub use completion::{CandidateProcessingOptions, Completions};
pub use error::Error;
pub use interp::ExecutionResult;
pub use shell::{CreateOptions, Shell};
