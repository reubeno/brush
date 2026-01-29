//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

pub mod arithmetic;
mod braceexpansion;
pub mod builtins;
pub mod callstack;
pub mod commands;
pub mod completion;
pub mod env;
pub mod error;
pub mod escape;
pub mod expansion;
mod extendedtests;
pub mod extensions;
/// Filter infrastructure for intercepting shell operations.
///
/// This module provides zero-cost abstractions for pre/post operation filtering,
/// enabling custom hooks for command execution, script sourcing, and more.
///
/// For architecture details and usage examples, see:
/// - [`filter`] module documentation
/// - `docs/reference/filter-architecture.md` in the repository
///
/// # Quick Start
///
/// ```ignore
/// use brush_core::filter::{CmdExecFilter, PreFilterResult, SimpleCmdParams};
///
/// #[derive(Clone, Default)]
/// struct MyFilter;
///
/// impl CmdExecFilter for MyFilter {
///     async fn pre_simple_cmd<'a, SE: ShellExtensions>(
///         &self,
///         params: SimpleCmdParams<'a, SE>,
///     ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
///         println!("Executing: {}", params.command_name());
///         PreFilterResult::Continue(params)
///     }
/// }
/// ```
pub mod filter;
pub mod functions;
pub mod history;
pub mod int_utils;
pub mod interfaces;
mod interp;
mod ioutils;
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
pub mod results;
mod shell;
pub mod sourceinfo;
pub mod sys;
pub mod terminal;
pub mod tests;
pub mod timing;
pub mod trace_categories;
pub mod traps;
pub mod variables;
mod wellknownvars;

/// Re-export parser types used in core definitions.
pub mod parser {
    pub use brush_parser::{
        BindingParseError, ParseError, SourcePosition, SourcePositionOffset, SourceSpan,
        TestCommandParseError, WordParseError, ast,
    };
}

pub use commands::{CommandArg, ExecutionContext};
pub use error::{BuiltinError, Error, ErrorKind};
pub use extensions::ShellExtensions;
pub use interp::{ExecutionParameters, ProcessGroupPolicy};
pub use parser::{SourcePosition, SourcePositionOffset, SourceSpan};
pub use results::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, ExecutionSpawnResult};
pub use shell::{
    CreateOptions, ProfileLoadBehavior, RcLoadBehavior, Shell, ShellBuilder, ShellBuilderState,
    ShellFd, ShellState,
};
pub use sourceinfo::SourceInfo;
pub use variables::{ShellValue, ShellVariable};
