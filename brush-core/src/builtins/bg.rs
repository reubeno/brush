use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Moves a job to run in the background.
#[derive(Parser)]
pub(crate) struct BgCommand {
    /// List of job specs to move to background.
    job_specs: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for BgCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut exit_code = BuiltinExitCode::Success;

        if !self.job_specs.is_empty() {
            for job_spec in &self.job_specs {
                if let Some(job) = context.shell.jobs.resolve_job_spec(job_spec) {
                    job.move_to_background()?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {}: no such job",
                        context.command_name,
                        job_spec
                    )?;
                    exit_code = BuiltinExitCode::Custom(1);
                }
            }
        } else {
            if let Some(job) = context.shell.jobs.current_job_mut() {
                job.move_to_background()?;
            } else {
                writeln!(context.stderr(), "{}: no current job", context.command_name)?;
                exit_code = BuiltinExitCode::Custom(1);
            }
        }

        Ok(exit_code)
    }
}
