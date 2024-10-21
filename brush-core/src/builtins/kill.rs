use clap::Parser;
use nix::sys::signal::Signal;
use std::io::Write;
use std::str::FromStr;

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
    // TODO: `0 EXIT` signal is missing. It is not in the posix spec, but it exists in Bash
    // https://man7.org/linux/man-pages/man7/signal.7.html
    // https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/signal.h.html
    if !signals.is_empty() {
        for s in signals {
            // If the user gives us a code, we print the name; if they give a name, we print its
            // code.
            enum Sigspec {
                Sigspec(&'static str),
                Signum(i32),
            }
            let signal = s
                .parse::<i32>()
                .ok()
                .map(|code| {
                    Signal::try_from(code)
                        .map(|s| {
                            // bash compatinility. `SIGHUP` -> `HUP`
                            Sigspec::Sigspec(s.as_str().strip_prefix("SIG").unwrap_or(s.as_str()))
                        })
                        .ok()
                })
                .flatten()
                .or_else(|| {
                    // bash compatibility:
                    // support for names without `SIG`, for example `HUP` -> `SIGHUP`
                    let mut sig_str = String::with_capacity(3 + s.len());
                    if s.len() >= 3 && s[..3] != *"SIG" {
                        sig_str.push_str("SIG");
                        sig_str.push_str(s.as_str());
                    } else {
                        sig_str.push_str(s.as_str());
                    }
                    Signal::from_str(sig_str.as_str())
                        .ok()
                        .map(|s| Sigspec::Signum(s as i32))
                });
            if let Some(signal) = signal {
                match signal {
                    Sigspec::Signum(n) => {
                        writeln!(context.stdout(), "{}", n)?;
                    }
                    Sigspec::Sigspec(s) => {
                        writeln!(context.stdout(), "{}", s)?;
                    }
                }
            } else {
                writeln!(
                    context.stderr(),
                    "{}: {}: invalid signal specification",
                    context.command_name,
                    s
                )?;
                exit_code = builtins::ExitCode::Custom(1);
            }
        }
    } else {
        for i in Signal::iterator() {
            writeln!(context.stdout(), "{}", i)?;
        }
    }

    Ok(exit_code)
}
