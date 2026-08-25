//! The `jobs` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(JobsCommand);

use brush_core::{ExecutionResult, error, jobs};
use std::io::Write;

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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &JobsCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.also_show_pids {
        return error::unimp("jobs -l");
    }
    if command.list_changed_only {
        return error::unimp("jobs -n");
    }

    if command.job_specs.is_empty() {
        for job in &context.shell.jobs().jobs {
            command.display_job(&context, job)?;
        }
    } else {
        return error::unimp("jobs with job specs");
    }

    Ok(ExecutionResult::success())
}
