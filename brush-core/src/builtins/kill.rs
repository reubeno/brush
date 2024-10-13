use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error};

/// Signal a job or process.
#[derive(Parser)]
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    #[arg(short = 's')]
    signal_name: Option<String>,

    /// Number of the signal to send.
    #[arg(short = 'n')]
    signal_number: Option<usize>,

    //
    // TODO: implement -sigspec syntax
    /// List known signal names.
    #[arg(short = 'l', short_alias = 'L')]
    list_signals: bool,

    // Interpretation of these depends on whether -l is present.
    args: Vec<String>,
}

impl builtins::Command for KillCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.signal_name.is_some() {
            return error::unimp("kill -s");
        }
        if self.signal_number.is_some() {
            return error::unimp("kill -n");
        }

        if self.list_signals {
            error::unimp("kill -l")
        } else {
            if self.args.len() != 1 {
                writeln!(context.stderr(), "{}: invalid usage", context.command_name)?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }

            let exit_code = builtins::ExitCode::Success;

            let pid_or_job_spec = &self.args[0];
            if pid_or_job_spec.starts_with('%') {
                // It's a job spec.
                if let Some(job) = context.shell.jobs.resolve_job_spec(pid_or_job_spec) {
                    job.kill()?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {}: no such job",
                        context.command_name,
                        pid_or_job_spec
                    )?;
                    return Ok(builtins::ExitCode::Custom(1));
                }
            } else {
                let pid = pid_or_job_spec.parse::<i32>()?;

                // It's a pid.
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::SIGKILL)
                    .map_err(|_errno| error::Error::FailedToSendSignal)?;
            }

            Ok(exit_code)
        }
    }
}
