//! `wait` builtin: `WaitCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error};
use brush_core::args::{ArgsError, FromArgs};

/// Wait for jobs to terminate.
#[derive(usage::Cli)]
#[usage(bin = "wait", unknown_flags = "error", args_override_self = false)]
pub(crate) struct WaitCommand {
    /// Wait for specified job to terminate (instead of change status).
    #[usage(short = 'f')]
    pub(super) wait_for_terminate: bool,

    /// Wait for a single job to change status; if jobs are specified, waits for
    /// the first to change status, and otherwise waits for the next change.
    #[usage(short = 'n')]
    pub(super) wait_for_first_or_next: bool,

    /// Name of variable to receive the job ID of the job whose status is indicated.
    #[usage(short = 'p', value_name = "VAR_NAME")]
    pub(super) variable_to_receive_id: Option<String>,

    /// Process IDs or job specs to wait for.
    pub(super) ids: Vec<String>,
}

crate::impl_usage_parse!(WaitCommand);

impl FromArgs for WaitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for WaitCommand {
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
