use clap::Parser;
use std::io::Write;

use crate::traps::TrapSignal;
use crate::{builtins, commands, error, sys};

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
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for KillCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // Default signal is SIGKILL.
        let mut trap_signal = TrapSignal::Signal(nix::sys::signal::Signal::SIGKILL);

        // Try parsing the signal name (if specified).
        if let Some(signal_name) = &self.signal_name {
            if let Ok(parsed_trap_signal) = TrapSignal::try_from(signal_name.as_str()) {
                trap_signal = parsed_trap_signal;
            } else {
                writeln!(
                    context.stderr(),
                    "{}: invalid signal name: {}",
                    context.command_name,
                    signal_name
                )?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }
        }

        // Try parsing the signal number (if specified).
        if let Some(signal_number) = &self.signal_number {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_possible_wrap)]
            if let Ok(parsed_trap_signal) = TrapSignal::try_from(*signal_number as i32) {
                trap_signal = parsed_trap_signal;
            } else {
                writeln!(
                    context.stderr(),
                    "{}: invalid signal number: {}",
                    context.command_name,
                    signal_number
                )?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }
        }

        // Look through the remaining args for a pid/job spec or a -sigspec style option.
        let mut pid_or_job_spec = None;
        for arg in &self.args {
            // See if this is -sigspec syntax.
            if let Some(possible_sigspec) = arg.strip_prefix("-") {
                // See if this is -sigspec syntax.
                if let Ok(parsed_trap_signal) = TrapSignal::try_from(possible_sigspec) {
                    trap_signal = parsed_trap_signal;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: invalid signal name",
                        context.command_name
                    )?;
                    return Ok(builtins::ExitCode::InvalidUsage);
                }
            } else if pid_or_job_spec.is_none() {
                pid_or_job_spec = Some(arg);
            } else {
                writeln!(
                    context.stderr(),
                    "{}: too many jobs or processes specified",
                    context.command_name
                )?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }
        }

        if self.list_signals {
            return print_signals(&context, self.args.as_ref());
        } else {
            if pid_or_job_spec.is_none() {
                writeln!(context.stderr(), "{}: invalid usage", context.command_name)?;
                return Ok(builtins::ExitCode::InvalidUsage);
            }

            let pid_or_job_spec = pid_or_job_spec.unwrap();
            if pid_or_job_spec.starts_with('%') {
                // It's a job spec.
                if let Some(job) = context.shell.jobs.resolve_job_spec(pid_or_job_spec) {
                    job.kill(trap_signal)?;
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
                sys::signal::kill_process(pid, trap_signal)?;
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
