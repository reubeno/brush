//! The `wait` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(WaitCommand);

use brush_core::{ExecutionExitCode, ExecutionResult, error};
use std::io::Write;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &WaitCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.wait_for_terminate {
        return error::unimp("wait -f");
    }
    if command.wait_for_first_or_next {
        return error::unimp("wait -n");
    }
    if command.variable_to_receive_id.is_some() {
        return error::unimp("wait -p");
    }

    let mut result = ExecutionResult::success();

    if !command.ids.is_empty() {
        for id in &command.ids {
            if id.starts_with('%') {
                // It's a job spec.
                if let Some(job) = context.shell.jobs_mut().resolve_job_spec(id) {
                    job.wait().await?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: no such job: {}",
                        context.command_name,
                        id
                    )?;

                    result = ExecutionExitCode::GeneralError.into();
                }
            } else {
                // It's a process ID.
                return error::unimp("wait with process IDs");
            }
        }
    } else {
        // Wait for all jobs.
        let jobs = context.shell.jobs_mut().wait_all().await?;

        if context.shell.options().enable_job_control {
            for job in jobs {
                writeln!(context.stdout(), "{job}")?;
            }
        }
    }

    Ok(result)
}
