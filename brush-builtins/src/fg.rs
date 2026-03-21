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
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let is_interactive = context.shell.options().interactive;

        let result = if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                run_job(job, is_interactive).await?
            } else {
                let mut stderr_output = Vec::new();
                writeln!(
                    stderr_output,
                    "{}: {}: no such job",
                    job_spec, context.command_name
                )?;
                context.stderr().write_all(&stderr_output)?;
                context.stderr().flush()?;
                ExecutionResult::general_error()
            }
        } else if let Some(job) = context.shell.jobs_mut().current_job_mut() {
            run_job(job, is_interactive).await?
        } else {
            let mut stderr_output = Vec::new();
            writeln!(stderr_output, "{}: no current job", context.command_name)?;
            context.stderr().write_all(&stderr_output)?;
            context.stderr().flush()?;
            ExecutionResult::general_error()
        };

        Ok(result)
    }
}

async fn run_job(
    job: &mut brush_core::jobs::Job,
    is_interactive: bool,
) -> Result<ExecutionResult, brush_core::Error> {
    job.move_to_foreground()?;

    eprintln!("{}", job.command_line);

    let result = job.wait().await?;
    if is_interactive {
        sys::terminal::move_self_to_foreground()?;
    }

    if matches!(job.state, jobs::JobState::Stopped) {
        let formatted = job.to_string();
        eprintln!("\r{formatted}");
    }

    Ok(result)
}
