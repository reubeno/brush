use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, jobs};

/// Moves a job to run in the background.
#[derive(Parser)]
pub(crate) struct BgCommand {
    /// List of job specs to move to background.
    job_specs: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(BgCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for BgCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        if !self.job_specs.is_empty() {
            for job_spec in &self.job_specs {
                if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                    let running = running_job_id(job);
                    job.move_to_background()?;
                    warn_if_running(&context, running)?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {}: no such job",
                        context.command_name,
                        job_spec
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        } else {
            if let Some(job) = context.shell.jobs_mut().current_job_mut() {
                let running = running_job_id(job);
                job.move_to_background()?;
                warn_if_running(&context, running)?;
            } else {
                writeln!(context.stderr(), "{}: no current job", context.command_name)?;
                exit_code = ExecutionResult::general_error();
            }
        }

        Ok(exit_code)
    }
}

/// Returns the ID of the job if it isn't known to be stopped. (`bg` continues such a job
/// anyway, as it may have been stopped without the shell's noticing.)
fn running_job_id(job: &jobs::Job) -> Option<usize> {
    (!matches!(job.state, jobs::JobState::Stopped)).then_some(job.id)
}

/// Warns, as bash does, that a job not known to be stopped is already in the background.
fn warn_if_running<SE: brush_core::ShellExtensions>(
    context: &brush_core::ExecutionContext<'_, SE>,
    running_job_id: Option<usize>,
) -> Result<(), brush_core::Error> {
    if let Some(id) = running_job_id {
        writeln!(
            context.stderr(),
            "{}: job {id} already in background",
            context.command_name
        )?;
    }
    Ok(())
}
