//! The `fg` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(FgCommand);

use brush_core::{ExecutionResult, jobs, sys};
use std::io::Write;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &FgCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut stderr = context.stdout();

    // Read interactive option before taking mutable borrow on jobs
    let is_interactive = context.shell.options().interactive;

    if let Some(job_spec) = &command.job_spec {
        if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
            job.move_to_foreground()?;
            writeln!(stderr, "{}", job.command_line)?;

            let result = job.wait().await?;
            if is_interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if matches!(job.state, jobs::JobState::Stopped) {
                // N.B. We use the '\r' to overwrite any ^Z output.
                let formatted = job.to_string();
                writeln!(context.stderr(), "\r{formatted}")?;
            }

            Ok(result)
        } else {
            writeln!(
                stderr,
                "{}: {}: no such job",
                job_spec, context.command_name
            )?;
            Ok(ExecutionResult::general_error())
        }
    } else {
        if let Some(job) = context.shell.jobs_mut().current_job_mut() {
            job.move_to_foreground()?;
            writeln!(stderr, "{}", job.command_line)?;

            let result = job.wait().await?;
            if is_interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if matches!(job.state, jobs::JobState::Stopped) {
                // N.B. We use the '\r' to overwrite any ^Z output.
                let formatted = job.to_string();
                writeln!(context.stderr(), "\r{formatted}")?;
            }

            Ok(result)
        } else {
            writeln!(stderr, "{}: no current job", context.command_name)?;
            Ok(ExecutionResult::general_error())
        }
    }
}
