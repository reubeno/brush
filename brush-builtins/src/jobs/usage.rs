//! `jobs` builtin: `JobsCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Manage jobs.
#[derive(usage::Cli)]
#[usage(bin = "jobs", unknown_flags = "error", args_override_self = false)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[usage(short = 'l')]
    pub(super) also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[usage(short = 'n')]
    pub(super) list_changed_only: bool,

    /// Show only process IDs.
    #[usage(short = 'p')]
    pub(super) show_pids_only: bool,

    /// Show only running jobs.
    #[usage(short = 'r')]
    pub(super) running_jobs_only: bool,

    /// Show only stopped jobs.
    #[usage(short = 's')]
    pub(super) stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO(jobs): Add -x option
    pub(super) job_specs: Vec<String>,
}

crate::impl_usage_parse!(JobsCommand);

impl FromArgs for JobsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for JobsCommand {
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
