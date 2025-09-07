//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

pub mod completion;

mod arithmetic;
mod braceexpansion;
pub mod builtins;
mod commands;
pub mod env;
mod error;
mod escape;
mod expansion;
mod extendedtests;
pub mod functions;
pub mod history;
pub mod interfaces;
mod interp;
mod jobs;
mod keywords;
mod namedoptions;
mod openfiles;
pub mod options;
mod pathcache;
mod pathsearch;
mod patterns;
mod alias_events;
mod builtin_events;
mod process_events;
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
pub use commands::{CommandArg, ExecutionContext};
pub use error::Error;
pub use interp::{ExecutionParameters, ExecutionResult, ProcessGroupPolicy};
pub use openfiles::{OpenFile, OpenFiles, OpenPipeReader, OpenPipeWriter, pipe};
pub use alias_events::{AliasEvent, set_alias_event_sender, emit as emit_alias_event};
pub use builtin_events::{BuiltinEvent, set_builtin_event_sender as set_builtin_event_sender, emit as emit_builtin_event, next_id as next_builtin_id};
pub use process_events::{ProcessEvent, set_process_event_sender};
pub use shell::{CreateOptions, Shell};
pub use terminal::{TerminalControl, get_foreground_pid};
pub use variables::{ShellValue, ShellVariable};
