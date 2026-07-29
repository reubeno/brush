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
    CreateOptions, ProfileLoadBehavior, RcLoadBehavior, Shell, ShellBuilder, ShellBuilderState,
    ShellFd, ShellState,
};
pub use sourceinfo::SourceInfo;
pub use variables::{ShellValue, ShellVariable};

/// Re-export of [`bstr::BString`] for downstream use.
pub use bstr::BString;

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

/// Converts an [`OsString`] to a [`bstr::BString`], preserving raw bytes on Unix
/// (so non-UTF-8 paths/environment values are not corrupted). On non-Unix
/// platforms a lossy conversion is used as a fallback.
pub fn os_string_to_bstring(value: std::ffi::OsString) -> bstr::BString {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            bstr::BString::new(value.into_vec())
        } else {
            bstr::BString::from(value.to_string_lossy().into_owned())
        }
    }
}

/// Converts a [`Path`] to a [`bstr::BString`], preserving raw bytes on Unix.
pub fn path_to_bstring(path: impl AsRef<std::path::Path>) -> bstr::BString {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            bstr::BString::new(path.as_ref().as_os_str().as_bytes().to_vec())
        } else {
            bstr::BString::from(path.as_ref().to_string_lossy().into_owned())
        }
    }
}
