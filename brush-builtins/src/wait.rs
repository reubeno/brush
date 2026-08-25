use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins, error,
};

/// Wait for jobs to terminate.
pub(crate) struct WaitCommand {
    wait_for_terminate: bool,
    wait_for_first_or_next: bool,
    variable_to_receive_id: Option<String>,
    ids: Vec<String>,
}

const ID_WAIT_FOR_TERMINATE: &str = "wait_for_terminate";
const ID_WAIT_FOR_FIRST_OR_NEXT: &str = "wait_for_first_or_next";
const ID_VARIABLE_TO_RECEIVE_ID: &str = "variable_to_receive_id";
const ID_IDS: &str = "ids";

impl builtins::SpecCommand for WaitCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[
                ArgSpec::flag(
                    ID_WAIT_FOR_TERMINATE,
                    &['f'],
                    &[],
                    "Wait for specified job to terminate (instead of change status).",
                ),
                ArgSpec::flag(
                    ID_WAIT_FOR_FIRST_OR_NEXT,
                    &['n'],
                    &[],
                    "Wait for a single job to change status; if jobs are specified, waits for the first to change status, and otherwise waits for the next change.",
                ),
                ArgSpec::value(
                    ID_VARIABLE_TO_RECEIVE_ID,
                    &['p'],
                    &[],
                    "VAR_NAME",
                    "Name of variable to receive the job ID of the job whose status is indicated.",
                ),
            ],
            positionals: &[PositionalSpec::many(ID_IDS, "IDS")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            wait_for_terminate: values.flag(ID_WAIT_FOR_TERMINATE),
            wait_for_first_or_next: values.flag(ID_WAIT_FOR_FIRST_OR_NEXT),
            variable_to_receive_id: values
                .value(ID_VARIABLE_TO_RECEIVE_ID)
                .map(ToOwned::to_owned),
            ids: values.positional_values(ID_IDS).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Wait for jobs to terminate."
    }

    fn synopsis() -> &'static str {
        "[-fn] [-p VAR_NAME] [IDS]..."
    }

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
}
