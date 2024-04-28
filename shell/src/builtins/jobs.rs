use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use crate::error;

#[derive(Parser)]
pub(crate) struct JobsCommand {
    #[arg(short = 'l')]
    also_show_pids: bool,

    #[arg(short = 'n')]
    list_changed_only: bool,

    #[arg(short = 'p')]
    show_pids_only: bool,

    #[arg(short = 'r')]
    running_jobs_only: bool,

    #[arg(short = 's')]
    stopped_jobs_only: bool,

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

        for job in &context.shell.jobs.background_jobs {
            let annotation = if job.is_current() {
                "+"
            } else if job.is_prev() {
                "-"
            } else {
                ""
            };

            writeln!(
                context.stdout(),
                "[{}]{:3}{:24}{}",
                job.id,
                annotation,
                job.state.to_string(),
                job.command_line
            )?;
        }

        Ok(BuiltinExitCode::Success)
    }
}
