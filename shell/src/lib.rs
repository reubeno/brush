#![deny(clippy::all)]
// #![deny(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::if_same_then_else)]

mod arithmetic;
mod builtin;
mod builtins;
mod context;
mod env;
mod expansion;
mod interp;
mod namedoptions;
mod options;
mod patterns;
mod prompt;
mod shell;
mod variables;

pub use interp::ExecutionResult;
pub use shell::{CreateOptions, Shell};
