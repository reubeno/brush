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
#[cfg(feature = "experimental-filters")]
pub mod filter;
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
pub use interp::{ExecutionParameters, ProcessGroupPolicy};
pub use parser::{SourcePosition, SourcePositionOffset, SourceSpan};
pub use results::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, ExecutionSpawnResult};
pub use shell::{CreateOptions, Shell, ShellBuilder, ShellBuilderState, ShellFd};
pub use sourceinfo::SourceInfo;
pub use variables::{ShellValue, ShellVariable};

#[cfg(feature = "experimental-filters")]
pub use shell::ScriptArgs;

/// No-op version of `with_filter!` when experimental-filters is disabled.
///
/// This macro expands directly to the body with zero overhead.
#[cfg(not(feature = "experimental-filters"))]
#[macro_export]
macro_rules! with_filter {
    ($shell:expr, $pre_method:ident, $post_method:ident, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        #[allow(clippy::redundant_locals)]
        let $input_ident = $input_val;
        $body
    }};
}
