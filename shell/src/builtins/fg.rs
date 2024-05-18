use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    job_spec: Option<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for FgCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut stderr = context.stdout();

        if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs.resolve_job_spec(job_spec) {
                job.move_to_foreground()?;
                writeln!(stderr, "{}", job.command_line)?;

                let result = job.wait().await?;
                Ok(BuiltinExitCode::from(result))
            } else {
                writeln!(
                    stderr,
                    "{}: {}: no such job",
                    job_spec, context.command_name
                )?;
                Ok(BuiltinExitCode::Custom(1))
            }
        } else {
            if let Some(job) = context.shell.jobs.current_job_mut() {
                job.move_to_foreground()?;
                writeln!(stderr, "{}", job.command_line)?;

                let result = job.wait().await?;
                Ok(BuiltinExitCode::from(result))
            } else {
                writeln!(stderr, "{}: no current job", context.command_name)?;
                Ok(BuiltinExitCode::Custom(1))
            }
        }
    }
}
