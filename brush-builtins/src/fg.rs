use brush_core::writeln_async;

use brush_core::{ExecutionResult, builtins, jobs, sys};

/// Move a specified job to the foreground.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    #[arg(positional)]
    job_spec: Option<String>,
}

brush_winnow_args_adapter::impl_parse_args!(FgCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for FgCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut stdout = context.stdout_async();

        // Read interactive option before taking mutable borrow on jobs
        let is_interactive = context.shell.options().interactive;

        if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                job.move_to_foreground()?;
                writeln_async!(stdout, "{}", job.command_line).await?;

                let result = job.wait().await?;
                if is_interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln_async!(context.stderr_async(), "\r{formatted}").await?;
                }

                Ok(result)
            } else {
                writeln_async!(
                    context.stderr_async(),
                    "{}: {}: no such job",
                    context.command_name,
                    job_spec
                )
                .await?;
                Ok(ExecutionResult::general_error())
            }
        } else {
            if let Some(job) = context.shell.jobs_mut().current_job_mut() {
                job.move_to_foreground()?;
                writeln_async!(stdout, "{}", job.command_line).await?;

                let result = job.wait().await?;
                if is_interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln_async!(context.stderr_async(), "\r{formatted}").await?;
                }

                Ok(result)
            } else {
                writeln_async!(
                    context.stderr_async(),
                    "{}: no current job",
                    context.command_name
                )
                .await?;
                Ok(ExecutionResult::general_error())
            }
        }
    }
}
