use clap::Parser;
use std::io::Write;

use crate::traps::TrapSignal;
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
            return print_signals(&context, self.args.as_ref());
        } else {
            if self.args.len() != 1 {
                writeln!(context.stderr(), "{}: invalid usage", context.command_name)?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }

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
        }
        Ok(builtins::ExitCode::Success)
    }
}

fn print_signals(
    context: &commands::ExecutionContext<'_>,
    signals: &[String],
) -> Result<builtins::ExitCode, error::Error> {
    let mut exit_code = builtins::ExitCode::Success;
    if !signals.is_empty() {
        for s in signals {
            // If the user gives us a code, we print the name; if they give a name, we print its
            // code.
            enum PrintSignal {
                Name(&'static str),
                Num(i32),
            }

            let signal = if let Ok(n) = s.parse::<i32>() {
                // bash compatibility. `SIGHUP` -> `HUP`
                TrapSignal::try_from(n).map(|s| {
                    PrintSignal::Name(s.as_str().strip_prefix("SIG").unwrap_or(s.as_str()))
                })
            } else {
                TrapSignal::try_from(s.as_str()).map(|sig| {
                    i32::try_from(sig).map_or(PrintSignal::Name(sig.as_str()), PrintSignal::Num)
                })
            };

            match signal {
                Ok(PrintSignal::Num(n)) => {
                    writeln!(context.stdout(), "{n}")?;
                }
                Ok(PrintSignal::Name(s)) => {
                    writeln!(context.stdout(), "{s}")?;
                }
                Err(e) => {
                    writeln!(context.stderr(), "{e}")?;
                    exit_code = builtins::ExitCode::Custom(1);
                }
            }
        }
    } else {
        return crate::traps::format_signals(
            context.stdout(),
            TrapSignal::iterator().filter(|s| !matches!(s, TrapSignal::Exit)),
        )
        .map(|()| builtins::ExitCode::Success);
    }

    Ok(exit_code)
}
