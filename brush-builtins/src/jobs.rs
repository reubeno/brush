use std::io::Write;

use brush_core::{ExecutionResult, builtins, error, jobs};

/// Manage jobs.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
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
    // TODO(jobs): Add -x option
    #[arg(positional)]
    job_specs: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(JobsCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for JobsCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.also_show_pids {
            return error::unimp("jobs -l");
        }
        if self.list_changed_only {
            return error::unimp("jobs -n");
        }

        // Buffer output
        let mut output = Vec::new();

        if self.job_specs.is_empty() {
            for job in &context.shell.jobs().jobs {
                self.format_job(&mut output, job)?;
            }
        } else {
            return error::unimp("jobs with job specs");
        }

        // Write output async
        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

impl JobsCommand {
    fn format_job(&self, output: &mut Vec<u8>, job: &jobs::Job) -> Result<(), brush_core::Error> {
        if self.running_jobs_only && !matches!(job.state, jobs::JobState::Running) {
            return Ok(());
        }
        if self.stopped_jobs_only && !matches!(job.state, jobs::JobState::Stopped) {
            return Ok(());
        }

        if self.show_pids_only {
            if let Some(pid) = job.representative_pid() {
                writeln!(output, "{pid}")?;
            }
        } else {
            writeln!(output, "{job}")?;
        }

        Ok(())
    }
}
