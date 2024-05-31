use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use crate::error;

/// Manage jobs.
#[derive(Parser)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[arg(short = 'l')]
    also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[arg(short = 'n')]
    list_changed_only: bool,

    /// Show only process IDs.
    #[arg(short = 'p')]
    show_pids_only: bool,

    /// Show only running jobs.
    #[arg(short = 'r')]
    running_jobs_only: bool,

    /// Show only stopped jobs.
    #[arg(short = 's')]
    stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO: Add -x option
    job_specs: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for JobsCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.also_show_pids {
            return error::unimp("jobs -l");
        }
        if self.list_changed_only {
            return error::unimp("jobs -n");
        }
        if self.show_pids_only {
            return error::unimp("jobs -p");
        }
        if self.running_jobs_only {
            return error::unimp("jobs -r");
        }
        if self.stopped_jobs_only {
            return error::unimp("jobs -s");
        }
        if !self.job_specs.is_empty() {
            return error::unimp("jobs with job specs");
        }

        for job in &context.shell.jobs.jobs {
            writeln!(context.stdout(), "{job}")?;
        }

        Ok(BuiltinExitCode::Success)
    }
}
