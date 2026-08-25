use bpaf::Bpaf;

use std::io::Write;

use brush_core::{ExecutionResult, builtins, error, jobs};

/// Manage jobs.
#[derive(Bpaf)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[bpaf(short('l'))]
    also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[bpaf(short('n'))]
    list_changed_only: bool,

    /// Show only process IDs.
    #[bpaf(short('p'))]
    show_pids_only: bool,

    /// Show only running jobs.
    #[bpaf(short('r'))]
    running_jobs_only: bool,

    /// Show only stopped jobs.
    #[bpaf(short('s'))]
    stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO(jobs): Add -x option
    #[bpaf(positional("JOB_SPECS"))]
    job_specs: Vec<String>,
}

impl builtins::Command for JobsCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        jobs_command()
    }

    fn about() -> &'static str {
        "Manage jobs."
    }

    fn synopsis() -> &'static str {
        "[-lnprs] [JOB_SPECS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.also_show_pids {
            return error::unimp("jobs -l");
        }
        if self.list_changed_only {
            return error::unimp("jobs -n");
        }

        if self.job_specs.is_empty() {
            for job in &context.shell.jobs().jobs {
                self.display_job(&context, job)?;
            }
        } else {
            return error::unimp("jobs with job specs");
        }

        Ok(ExecutionResult::success())
    }
}

impl JobsCommand {
    fn display_job(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        job: &jobs::Job,
    ) -> Result<(), brush_core::Error> {
        if self.running_jobs_only && !matches!(job.state, jobs::JobState::Running) {
            return Ok(());
        }
        if self.stopped_jobs_only && !matches!(job.state, jobs::JobState::Stopped) {
            return Ok(());
        }

        if self.show_pids_only {
            if let Some(pid) = job.representative_pid() {
                writeln!(context.stdout(), "{pid}")?;
            }
        } else {
            writeln!(context.stdout(), "{job}")?;
        }

        Ok(())
    }
}
