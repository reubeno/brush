use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, jobs, sys};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    job_spec: Option<String>,
}

impl builtins::Command for FgCommand {
    type State = ();
    type SharedState = ();
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Grab the output handles up front: they're owned ('static) so they don't
        // hold a borrow of the shell, which we need to mutate via jobs_mut().
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let is_interactive = context.shell.options().interactive;

        let result = if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                run_job(job, is_interactive, &mut stdout, &mut stderr).await?
            } else {
                writeln!(
                    stderr,
                    "{}: {}: no such job",
                    job_spec, context.command_name
                )?;
                ExecutionResult::general_error()
            }
        } else if let Some(job) = context.shell.jobs_mut().current_job_mut() {
            run_job(job, is_interactive, &mut stdout, &mut stderr).await?
        } else {
            writeln!(stderr, "{}: no current job", context.command_name)?;
            ExecutionResult::general_error()
        };

        stdout.flush()?;
        stderr.flush()?;
        Ok(result)
    }
}

async fn run_job(
    job: &mut brush_core::jobs::Job,
    is_interactive: bool,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<ExecutionResult, brush_core::Error> {
    job.move_to_foreground()?;

    writeln!(stdout, "{}", job.command_line)?;

    let result = job.wait().await?;
    if is_interactive {
        sys::terminal::move_self_to_foreground()?;
    }

    if matches!(job.state, jobs::JobState::Stopped) {
        // N.B. We use the '\r' to overwrite any ^Z output.
        let formatted = job.to_string();
        writeln!(stderr, "\r{formatted}")?;
    }

    Ok(result)
}
