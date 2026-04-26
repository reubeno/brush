use clap::Parser;

use std::io::Write;

use brush_core::{ExecutionResult, builtins, jobs, openfiles, sys};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    job_spec: Option<String>,
}

impl builtins::Command for FgCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let is_interactive = context.shell.options().interactive;
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();

        let result = if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                run_job(job, is_interactive, &mut stdout, &mut stderr).await?
            } else {
                let mut buf = Vec::new();
                writeln!(buf, "{}: {}: no such job", job_spec, context.command_name)?;
                write_to(&mut stderr, &buf).await;
                ExecutionResult::general_error()
            }
        } else if let Some(job) = context.shell.jobs_mut().current_job_mut() {
            run_job(job, is_interactive, &mut stdout, &mut stderr).await?
        } else {
            let mut buf = Vec::new();
            writeln!(buf, "{}: no current job", context.command_name)?;
            write_to(&mut stderr, &buf).await;
            ExecutionResult::general_error()
        };

        Ok(result)
    }
}

async fn write_to(file: &mut Option<openfiles::OpenFile>, buf: &[u8]) {
    if let Some(f) = file.as_mut() {
        let _ = f.write_all(buf).await;
        let _ = f.flush().await;
    }
}

async fn run_job(
    job: &mut brush_core::jobs::Job,
    is_interactive: bool,
    stdout: &mut Option<openfiles::OpenFile>,
    stderr: &mut Option<openfiles::OpenFile>,
) -> Result<ExecutionResult, brush_core::Error> {
    job.move_to_foreground()?;

    let mut buf = Vec::new();
    writeln!(buf, "{}", job.command_line)?;
    write_to(stdout, &buf).await;

    let result = job.wait().await?;
    if is_interactive {
        sys::terminal::move_self_to_foreground()?;
    }

    if matches!(job.state, jobs::JobState::Stopped) {
        let mut buf = Vec::new();
        writeln!(buf, "\r{job}")?;
        write_to(stderr, &buf).await;
    }

    Ok(result)
}
