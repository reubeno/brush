mod builtin;
mod builtins;
mod context;
mod expansion;
mod interp;
mod patterns;
mod prompt;
mod shell;

pub use interp::ExecutionResult;
pub use shell::{Shell, ShellCreateOptions};
