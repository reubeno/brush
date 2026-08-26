//! `read` builtin: `ReadCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools;
use brush_core::builtins;
use std::io::{Read, Write};
use brush_core::args::{ArgsError, FromArgs};

/// Parse standard input.
#[derive(usage::Cli)]
#[usage(bin = "read", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ReadCommand {
    /// Optionally, name of an array variable to receive read words
    /// of input.
    #[usage(short = 'a', value_name = "VAR_NAME")]
    pub(super) array_variable: Option<String>,

    /// Optionally, a delimiter to use other than a newline character.
    #[usage(short = 'd')]
    pub(super) delimiter: Option<String>,

    /// Use readline-like input.
    #[usage(short = 'e')]
    pub(super) use_readline: bool,

    /// Provide text to use as initial input for readline.
    #[usage(short = 'i', value_name = "STR")]
    pub(super) initial_text: Option<String>,

    /// Read only the first N characters or until a specified
    /// delimiter is reached, whichever happens first.
    #[usage(short = 'n', value_name = "COUNT")]
    pub(super) return_after_n_chars: Option<usize>,

    /// Read exactly N characters, ignoring any specified delimiter.
    #[usage(short = 'N', value_name = "COUNT")]
    pub(super) return_after_n_chars_no_delimiter: Option<usize>,

    /// Prompt to display before reading.
    #[usage(short = 'p')]
    pub(super) prompt: Option<String>,

    /// Read input in raw mode; no escape sequences.
    #[usage(short = 'r')]
    pub(super) raw_mode: bool,

    /// Do not echo input.
    #[usage(short = 's')]
    pub(super) silent: bool,

    /// Specify timeout in seconds; fail if the timeout elapses before
    /// input is completed.
    #[usage(short = 't', value_name = "SECONDS", allow_hyphen_values)]
    pub(super) timeout_in_seconds: Option<f64>,

    /// File descriptor to read from instead of stdin.
    #[usage(short = 'u', value_name = "FD")]
    pub(super) fd_num_to_read: Option<u8>,

    /// Optionally, names of variables to receive read input.
    pub(super) variable_names: Vec<String>,
}

crate::impl_usage_parse!(ReadCommand);

impl FromArgs for ReadCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ReadCommand {
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
