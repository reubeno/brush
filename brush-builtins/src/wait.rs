use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, error};

/// Wait for jobs to terminate.
#[derive(Parser)]
pub(crate) struct WaitCommand {
    /// Wait for specified job to terminate (instead of change status).
    #[arg(short = 'f')]
    wait_for_terminate: bool,

    /// Wait for a single job to change status; if jobs are specified, waits for
    /// the first to change status, and otherwise waits for the next change.
    #[arg(short = 'n')]
    wait_for_first_or_next: bool,

    /// Name of variable to receive the job ID of the job whose status is indicated.
    #[arg(short = 'p', value_name = "VAR_NAME")]
    variable_to_receive_id: Option<String>,

    /// Specs of jobs to wait for.
    job_specs: Vec<String>,
}

impl builtins::Command for WaitCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.wait_for_terminate {
            return error::unimp("wait -f");
        }
        if self.wait_for_first_or_next {
            return error::unimp("wait -n");
        }
        if self.variable_to_receive_id.is_some() {
            return error::unimp("wait -p");
        }
        if !self.job_specs.is_empty() {
            return error::unimp("wait with job specs");
        }

        let jobs = context.shell.jobs.wait_all().await?;

        if context.shell.options.enable_job_control {
            for job in jobs {
                writeln!(context.stdout(), "{job}")?;
            }
        }

        Ok(ExecutionResult::success())
    }
}
