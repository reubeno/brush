//! `jobs` builtin: `JobsCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage jobs.
#[derive(Parser)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[arg(short = 'l')]
    pub(super) also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[arg(short = 'n')]
    pub(super) list_changed_only: bool,

    /// Show only process IDs.
    #[arg(short = 'p')]
    pub(super) show_pids_only: bool,

    /// Show only running jobs.
    #[arg(short = 'r')]
    pub(super) running_jobs_only: bool,

    /// Show only stopped jobs.
    #[arg(short = 's')]
    pub(super) stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO(jobs): Add -x option
    pub(super) job_specs: Vec<String>,
}

impl builtins::Command for JobsCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
