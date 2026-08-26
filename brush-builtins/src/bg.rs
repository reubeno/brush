//! The `bg` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(BgCommand);

use brush_core::ExecutionResult;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &BgCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut exit_code = ExecutionResult::success();

    if !command.job_specs.is_empty() {
        for job_spec in &command.job_specs {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                job.move_to_background()?;
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
            job.move_to_background()?;
        } else {
            writeln!(context.stderr(), "{}: no current job", context.command_name)?;
            exit_code = ExecutionResult::general_error();
        }
    }

    Ok(exit_code)
}
