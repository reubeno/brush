use std::io::Write;

use brush_core::{ExecutionResult, builtins};

/// Moves a job to run in the background.
pub(crate) struct BgCommand {
    job_specs: Vec<String>,
}

const ID_JOB_SPECS: &str = "job_specs";

impl builtins::SpecCommand for BgCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.positional_many(ID_JOB_SPECS, "JOB_SPECS")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            job_specs: matches.values(ID_JOB_SPECS).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Moves a job to run in the background."
    }

    fn synopsis() -> &'static str {
        "[JOB_SPECS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        if !self.job_specs.is_empty() {
            for job_spec in &self.job_specs {
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
}
