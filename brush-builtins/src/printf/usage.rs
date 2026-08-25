//! `printf` builtin: `PrintfCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::{ffi::OsString, io::Write, ops::ControlFlow};
use uucore::format;
use brush_core::{Error, ErrorKind, ExecutionResult, builtins, escape, expansion};
use brush_core::args::{ArgsError, FromArgs};

/// Format a string.
#[derive(usage::Cli)]
#[usage(
    bin = "printf",
    unknown_flags = "error",
    args_override_self = false,
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[usage(short = 'v')]
    pub(super) output_variable: Option<String>,

    /// Format string + arguments to the format string.
    ///
    /// N.B. We intentionally do *not* enable `allow_hyphen_values` here. Doing so would
    /// cause an attached short-option value such as `-va` (i.e. `-v a`) to be misparsed as
    /// a positional argument. With it disabled, a format string that genuinely needs to
    /// start with a hyphen must be preceded by `--`, matching other shells' behavior.
    #[usage(trailing_var_arg, required)]
    pub(super) format_and_args: Vec<String>,
}

crate::impl_usage_parse!(PrintfCommand);

impl FromArgs for PrintfCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for PrintfCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
