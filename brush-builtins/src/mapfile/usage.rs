//! `mapfile` builtin: `MapFileCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::{Read, Write};
use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, env, error, variables};
use brush_core::args::{ArgsError, FromArgs};

/// Read lines from standard input into an indexed array variable.
#[derive(usage::Cli)]
#[usage(bin = "mapfile", unknown_flags = "error", args_override_self = false)]
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    #[usage(short = 'd')]
    pub(super) delimiter: Option<String>,

    /// Maximum number of entries to read (0 means no limit).
    #[usage(short = 'n', default = "0")]
    pub(super) max_count: i64,

    /// Index into array at which to start assignment.
    #[usage(short = 'O', allow_hyphen_values)]
    pub(super) origin: Option<i64>,

    /// Number of initial entries to skip.
    #[usage(short = 's', default = "0")]
    pub(super) skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    #[usage(short = 't')]
    pub(super) remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    #[usage(short = 'u', default = "0")]
    pub(super) fd: brush_core::ShellFd,

    /// Name of function to call for each group of lines.
    #[usage(short = 'C')]
    pub(super) callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    #[usage(short = 'c', default = "5000")]
    pub(super) callback_group_size: i64,

    /// Name of array to read into.
    #[usage(default = "MAPFILE")]
    pub(super) array_var_name: String,
}

crate::impl_usage_parse!(MapFileCommand);

impl FromArgs for MapFileCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for MapFileCommand {
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
