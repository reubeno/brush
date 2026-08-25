use std::io::Write;

use brush_core::{ExecutionResult, builtins, error, jobs};

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

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_ALSO_SHOW_PIDS,
            &['l'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Also show process IDs.",
        )
        .arg(
            ID_LIST_CHANGED_ONLY,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "List only jobs that have changed status since the last notification.",
        )
        .arg(
            ID_SHOW_PIDS_ONLY,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Show only process IDs.",
        )
        .arg(
            ID_RUNNING_JOBS_ONLY,
            &['r'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Show only running jobs.",
        )
        .arg(
            ID_STOPPED_JOBS_ONLY,
            &['s'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Show only stopped jobs.",
        )
        // TODO(jobs): Add -x option
        .positional_many(ID_JOB_SPECS, "JOB_SPECS")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            also_show_pids: matches.flag(ID_ALSO_SHOW_PIDS),
            list_changed_only: matches.flag(ID_LIST_CHANGED_ONLY),
            show_pids_only: matches.flag(ID_SHOW_PIDS_ONLY),
            running_jobs_only: matches.flag(ID_RUNNING_JOBS_ONLY),
            stopped_jobs_only: matches.flag(ID_STOPPED_JOBS_ONLY),
            job_specs: matches.values(ID_JOB_SPECS).to_vec(),
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
