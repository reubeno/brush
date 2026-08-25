//! `jobs` builtin: `JobsCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Manage jobs.
#[derive(Bpaf)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[bpaf(short('l'))]
    pub(super) also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[bpaf(short('n'))]
    pub(super) list_changed_only: bool,

    /// Show only process IDs.
    #[bpaf(short('p'))]
    pub(super) show_pids_only: bool,

    /// Show only running jobs.
    #[bpaf(short('r'))]
    pub(super) running_jobs_only: bool,

    /// Show only stopped jobs.
    #[bpaf(short('s'))]
    pub(super) stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO(jobs): Add -x option
    #[bpaf(positional("JOB_SPECS"))]
    pub(super) job_specs: Vec<String>,
}

impl crate::args::BpafArgs for JobsCommand {
fn parser() -> impl bpaf::Parser<Self> {
        jobs_command()
    }
fn about() -> &'static str {
        "Manage jobs."
    }
fn synopsis() -> &'static str {
        "[-lnprs] [JOB_SPECS]..."
    }
}

impl FromArgs for JobsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for JobsCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
