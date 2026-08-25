use std::io::Write;

use brush_core::{ExecutionResult, builtins, jobs, sys};

/// Move a specified job to the foreground.
pub(crate) struct FgCommand {
    job_spec: Option<String>,
}

const ID_JOB_SPEC: &str = "job_spec";

impl builtins::SpecCommand for FgCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.positional(ID_JOB_SPEC, "JOB_SPEC")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            job_spec: matches.value(ID_JOB_SPEC).map(ToOwned::to_owned),
        })
    }

    fn about() -> &'static str {
        "Move a specified job to the foreground."
    }

    fn synopsis() -> &'static str {
        "[JOB_SPEC]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut stderr = context.stdout();

        // Read interactive option before taking mutable borrow on jobs
        let is_interactive = context.shell.options().interactive;

        if let Some(job_spec) = &self.job_spec {
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
}
