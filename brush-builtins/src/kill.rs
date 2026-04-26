use clap::Parser;
use std::io::Write;

use brush_core::traps::TrapSignal;
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, sys};

/// Signal a job or process.
#[derive(Parser)]
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    #[arg(short = 's', value_name = "SIG_NAME")]
    signal_name: Option<String>,

    /// Number of the signal to send.
    #[arg(short = 'n', value_name = "SIG_NUM")]
    signal_number: Option<usize>,

    //
    // TODO(kill): implement -sigspec syntax
    /// List known signal names.
    #[arg(short = 'l', short_alias = 'L')]
    list_signals: bool,

    // Interpretation of these depends on whether -l is present.
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for KillCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut trap_signal = TrapSignal::Signal(nix::sys::signal::Signal::SIGKILL);

        if let Some(signal_name) = &self.signal_name {
            if let Ok(parsed_trap_signal) = TrapSignal::try_from(signal_name.as_str()) {
                trap_signal = parsed_trap_signal;
            } else {
                return write_error(
                    &context,
                    &format!(
                        "{}: invalid signal name: {}",
                        context.command_name, signal_name
                    ),
                    ExecutionExitCode::InvalidUsage,
                )
                .await;
            }
        }

        if let Some(signal_number) = &self.signal_number {
            #[expect(clippy::cast_possible_truncation)]
            #[expect(clippy::cast_possible_wrap)]
            if let Ok(parsed_trap_signal) = TrapSignal::try_from(*signal_number as i32) {
                trap_signal = parsed_trap_signal;
            } else {
                return write_error(
                    &context,
                    &format!(
                        "{}: invalid signal number: {}",
                        context.command_name, signal_number
                    ),
                    ExecutionExitCode::InvalidUsage,
                )
                .await;
            }
        }

        let mut pid_or_job_spec = None;
        for arg in &self.args {
            if let Some(possible_sigspec) = arg.strip_prefix("-") {
                if let Ok(parsed_trap_signal) = TrapSignal::try_from(possible_sigspec) {
                    trap_signal = parsed_trap_signal;
                } else {
                    return write_error(
                        &context,
                        &format!("{}: invalid signal name", context.command_name),
                        ExecutionExitCode::InvalidUsage,
                    )
                    .await;
                }
            } else if pid_or_job_spec.is_none() {
                pid_or_job_spec = Some(arg);
            } else {
                return write_error(
                    &context,
                    &format!(
                        "{}: too many jobs or processes specified",
                        context.command_name
                    ),
                    ExecutionExitCode::InvalidUsage,
                )
                .await;
            }
        }

        if self.list_signals {
            return print_signals(&context, self.args.as_ref()).await;
        } else {
            let Some(pid_or_job_spec) = pid_or_job_spec else {
                return write_error(
                    &context,
                    &format!("{}: invalid usage", context.command_name),
                    ExecutionExitCode::InvalidUsage,
                )
                .await;
            };

            if pid_or_job_spec.starts_with('%') {
                if let Some(job) = context.shell.jobs_mut().resolve_job_spec(pid_or_job_spec) {
                    job.kill(trap_signal)?;
                } else {
                    return write_error(
                        &context,
                        &format!("{}: {}: no such job", context.command_name, pid_or_job_spec),
                        ExecutionExitCode::GeneralError,
                    )
                    .await;
                }
            } else {
                let pid = brush_core::int_utils::parse(pid_or_job_spec.as_str(), 10)?;
                sys::signal::kill_process(pid, trap_signal)?;
            }
        }
        Ok(ExecutionResult::success())
    }
}

async fn write_error<SE: brush_core::ShellExtensions>(
    context: &brush_core::ExecutionContext<'_, SE>,
    message: &str,
    exit_code: ExecutionExitCode,
) -> Result<ExecutionResult, brush_core::Error> {
    let mut stderr_output = Vec::new();
    writeln!(stderr_output, "{message}")?;
    if let Some(mut stderr) = context.stderr() {
        stderr.write_all(&stderr_output).await?;
        stderr.flush().await?;
    }
    Ok(exit_code.into())
}

async fn print_signals(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    signals: &[String],
) -> Result<ExecutionResult, brush_core::Error> {
    let mut exit_code = ExecutionResult::success();
    let mut output = Vec::new();
    let mut stderr_output = Vec::new();

    if !signals.is_empty() {
        for s in signals {
            enum PrintSignal {
                Name(&'static str),
                Num(i32),
            }

            let signal = if let Ok(n) = s.parse::<i32>() {
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
                    writeln!(output, "{n}")?;
                }
                Ok(PrintSignal::Name(s)) => {
                    writeln!(output, "{s}")?;
                }
                Err(e) => {
                    writeln!(stderr_output, "{e}")?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }
    } else {
        let result = brush_core::traps::format_signals(
            &mut output,
            TrapSignal::iterator().filter(|s| !matches!(s, TrapSignal::Exit)),
        );
        if result.is_err() {
            return result.map(|()| ExecutionResult::success());
        }
    }

    if !output.is_empty() {
        if let Some(mut stdout) = context.stdout() {
            stdout.write_all(&output).await?;
            stdout.flush().await?;
        }
    }

    if !stderr_output.is_empty() {
        if let Some(mut stderr) = context.stderr() {
            stderr.write_all(&stderr_output).await?;
            stderr.flush().await?;
        }
    }

    Ok(exit_code)
}
