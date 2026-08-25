use std::io::Write;

use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins, error, jobs,
};

/// Manage jobs.
pub(crate) struct JobsCommand {
    also_show_pids: bool,
    list_changed_only: bool,
    show_pids_only: bool,
    running_jobs_only: bool,
    stopped_jobs_only: bool,
    job_specs: Vec<String>,
}

const ID_ALSO_SHOW_PIDS: &str = "also_show_pids";
const ID_LIST_CHANGED_ONLY: &str = "list_changed_only";
const ID_SHOW_PIDS_ONLY: &str = "show_pids_only";
const ID_RUNNING_JOBS_ONLY: &str = "running_jobs_only";
const ID_STOPPED_JOBS_ONLY: &str = "stopped_jobs_only";
const ID_JOB_SPECS: &str = "job_specs";

impl builtins::SpecCommand for JobsCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        // TODO(jobs): Add -x option
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[
                ArgSpec::flag(ID_ALSO_SHOW_PIDS, &['l'], &[], "Also show process IDs."),
                ArgSpec::flag(
                    ID_LIST_CHANGED_ONLY,
                    &['n'],
                    &[],
                    "List only jobs that have changed status since the last notification.",
                ),
                ArgSpec::flag(ID_SHOW_PIDS_ONLY, &['p'], &[], "Show only process IDs."),
                ArgSpec::flag(ID_RUNNING_JOBS_ONLY, &['r'], &[], "Show only running jobs."),
                ArgSpec::flag(ID_STOPPED_JOBS_ONLY, &['s'], &[], "Show only stopped jobs."),
            ],
            positionals: &[PositionalSpec::many(ID_JOB_SPECS, "JOB_SPECS")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            also_show_pids: values.flag(ID_ALSO_SHOW_PIDS),
            list_changed_only: values.flag(ID_LIST_CHANGED_ONLY),
            show_pids_only: values.flag(ID_SHOW_PIDS_ONLY),
            running_jobs_only: values.flag(ID_RUNNING_JOBS_ONLY),
            stopped_jobs_only: values.flag(ID_STOPPED_JOBS_ONLY),
            job_specs: values.positional_values(ID_JOB_SPECS).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Manage jobs."
    }

    fn synopsis() -> &'static str {
        "[-lnprs] [JOB_SPECS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.also_show_pids {
            return error::unimp("jobs -l");
        }
        if self.list_changed_only {
            return error::unimp("jobs -n");
        }

        if self.job_specs.is_empty() {
            for job in &context.shell.jobs().jobs {
                self.display_job(&context, job)?;
            }
        } else {
            return error::unimp("jobs with job specs");
        }

        Ok(ExecutionResult::success())
    }
}

impl JobsCommand {
    fn display_job(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        job: &jobs::Job,
    ) -> Result<(), brush_core::Error> {
        if self.running_jobs_only && !matches!(job.state, jobs::JobState::Running) {
            return Ok(());
        }
        if self.stopped_jobs_only && !matches!(job.state, jobs::JobState::Stopped) {
            return Ok(());
        }

        if self.show_pids_only {
            if let Some(pid) = job.representative_pid() {
                writeln!(context.stdout(), "{pid}")?;
            }
        } else {
            writeln!(context.stdout(), "{job}")?;
        }

        Ok(())
    }
}
