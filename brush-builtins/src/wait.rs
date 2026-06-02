use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error};

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

    /// Process IDs or job specs to wait for.
    ids: Vec<String>,
}

impl builtins::Command for WaitCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
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

        let mut result = ExecutionResult::success();

        if !self.ids.is_empty() {
            for id in &self.ids {
                if id.starts_with('%') {
                    // It's a job spec.
                    if let Some(job) = context.shell.jobs_mut().resolve_job_spec(id) {
                        // `wait` exits with the status of the job it waited on
                        // (the last one wins, matching bash).
                        result = job.wait().await?;
                    } else {
                        writeln!(
                            context.stderr(),
                            "{}: no such job: {}",
                            context.command_name,
                            id
                        )?;

                        // bash returns 127 for an unknown job spec.
                        result = ExecutionExitCode::from(127).into();
                    }
                } else {
                    // It's a process ID.
                    return error::unimp("wait with process IDs");
                }
            }
        } else {
            // Wait for all jobs. `wait` itself produces no output; completed jobs
            // are reported/cleared below.
            context.shell.jobs_mut().wait_all().await?;
        }

        // Clear the jobs we waited on. bash leaves no jobs behind after `wait`.
        if context.shell.reports_completed_jobs_at_prompt() {
            // Top-level interactive: report and clear completed jobs now -- bash
            // clears them when `wait` returns. Doing it here, rather than leaving
            // it to the next prompt cycle, avoids a stale-job window and a
            // duplicate `Done` notice within a single command line.
            context.shell.check_for_completed_jobs()?;
        } else {
            // No prompt cycle here (non-interactive scripts, `-c`, script files,
            // subshells): clear completed jobs silently so they don't linger.
            // `poll` also drains jobs that finished without being waited on.
            context.shell.jobs_mut().poll()?;
        }

        Ok(result)
    }
}
