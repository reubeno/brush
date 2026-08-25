//! `fg` builtin: `FgCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::{ExecutionResult, builtins, jobs, sys};
use brush_core::args::{ArgsError, FromArgs};

/// Move a specified job to the foreground.
#[derive(usage::Cli)]
#[usage(bin = "fg", unknown_flags = "error", args_override_self = false)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    pub(super) job_spec: Option<String>,
}

crate::impl_usage_parse!(FgCommand);

impl FromArgs for FgCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for FgCommand {
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
