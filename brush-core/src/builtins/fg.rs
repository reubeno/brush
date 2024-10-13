use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, jobs, sys};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    job_spec: Option<String>,
}

impl builtins::Command for FgCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut stderr = context.stdout();

        if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs.resolve_job_spec(job_spec) {
                job.move_to_foreground()?;
                writeln!(stderr, "{}", job.command_line)?;

                let result = job.wait().await?;
                if context.shell.options.interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln!(context.stderr(), "\r{formatted}")?;
                }

                Ok(builtins::ExitCode::from(result))
            } else {
                writeln!(
                    stderr,
                    "{}: {}: no such job",
                    job_spec, context.command_name
                )?;
                Ok(builtins::ExitCode::Custom(1))
            }
        } else {
            if let Some(job) = context.shell.jobs.current_job_mut() {
                job.move_to_foreground()?;
                writeln!(stderr, "{}", job.command_line)?;

                let result = job.wait().await?;
                if context.shell.options.interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln!(context.stderr(), "\r{formatted}")?;
                }

                Ok(builtins::ExitCode::from(result))
            } else {
                writeln!(stderr, "{}: no current job", context.command_name)?;
                Ok(builtins::ExitCode::Custom(1))
            }
        }
    }
}
