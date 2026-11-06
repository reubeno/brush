//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

pub mod arithmetic;
mod braceexpansion;
pub mod builtins;
mod cache;
pub mod callstack;
pub mod commands;
pub mod completion;
pub mod env;
pub mod error;
pub mod escape;
pub mod expansion;
mod extendedtests;
pub mod extensions;
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
pub mod tasks;
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
        BindingParseError, ParseError, ParserImpl, SourcePosition, SourcePositionOffset,
        SourceSpan, TestCommandParseError, WordParseError, ast,
    };
}

pub use commands::{CommandArg, ExecutionContext};
pub use error::{BuiltinError, Error, ErrorKind};
pub use extensions::ShellExtensions;
pub use interp::{ExecutionParameters, ProcessGroupPolicy};
pub use parser::{SourcePosition, SourcePositionOffset, SourceSpan};
pub use results::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, ExecutionSpawnResult};
pub use shell::{
    CreateOptions, ProfileLoadBehavior, RcLoadBehavior, SavedCommandStatus, Shell, ShellBuilder,
    ShellBuilderState, ShellFd, ShellState,
};
pub use sourceinfo::SourceInfo;
pub use variables::{ShellValue, ShellVariable};

/// Writes formatted text to an [`AsyncOpenFile`](openfiles::async_file::AsyncOpenFile),
/// as `write!` does to a synchronous writer; await the result.
///
/// The text is formatted before anything is awaited, so the future holds only the
/// formatted text, not the values it was formatted from, and stays `Send`. (`write!`
/// itself wouldn't: what it formats from lives until its statement ends, across the
/// await.)
///
/// ```ignore
/// brush_core::write_async!(context.stdout_async(), "{name}={value}").await?;
/// ```
#[macro_export]
macro_rules! write_async {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_text(::std::format!($($arg)*))
    };
}

/// Writes formatted text and a newline to an
/// [`AsyncOpenFile`](openfiles::async_file::AsyncOpenFile), as `writeln!` does to a
/// synchronous writer; await the result. See [`write_async!`].
#[macro_export]
macro_rules! writeln_async {
    ($dst:expr $(,)?) => {
        $dst.write_text("\n")
    };
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_text({
            let mut text = ::std::format!($($arg)*);
            text.push('\n');
            text
        })
    };
}
