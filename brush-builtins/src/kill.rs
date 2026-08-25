//! The `kill` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(KillCommand);

use brush_core::traps::TrapSignal;
use brush_core::{ExecutionExitCode, ExecutionResult, sys};
use std::io::Write;

pub(super) fn print_signals(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    signals: &[String],
) -> Result<ExecutionResult, brush_core::Error> {
    let mut exit_code = ExecutionResult::success();
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
                    exit_code = ExecutionResult::general_error();
                }
            }
        }
    } else {
        return brush_core::traps::format_signals(
            context.stdout(),
            TrapSignal::iterator().filter(|s| !matches!(s, TrapSignal::Exit)),
        )
        .map(|()| ExecutionResult::success());
    }

    Ok(exit_code)
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &KillCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // Default signal is SIGKILL.
    let mut trap_signal = TrapSignal::Signal(nix::sys::signal::Signal::SIGKILL);

    // Try parsing the signal name (if specified).
    if let Some(signal_name) = &command.signal_name {
        if let Ok(parsed_trap_signal) = TrapSignal::try_from(signal_name.as_str()) {
            trap_signal = parsed_trap_signal;
        } else {
            writeln!(
                context.stderr(),
                "{}: invalid signal name: {}",
                context.command_name,
                signal_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }
    }

    // Try parsing the signal number (if specified).
    if let Some(signal_number) = &command.signal_number {
        #[expect(clippy::cast_possible_truncation)]
        #[expect(clippy::cast_possible_wrap)]
        if let Ok(parsed_trap_signal) = TrapSignal::try_from(*signal_number as i32) {
            trap_signal = parsed_trap_signal;
        } else {
            writeln!(
                context.stderr(),
                "{}: invalid signal number: {}",
                context.command_name,
                signal_number
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }
    }

    // Look through the remaining args for a pid/job spec or a -sigspec style option.
    let mut pid_or_job_spec = None;
    for arg in &command.args {
        if let Some(possible_sigspec) = arg.strip_prefix("-") {
            // See if this is -sigspec syntax. The sigspec may be a signal name
            // (e.g., -TERM) or a signal number (e.g., -9).
            if let Ok(parsed_trap_signal) = possible_sigspec.parse::<TrapSignal>() {
                trap_signal = parsed_trap_signal;
            } else {
                writeln!(
                    context.stderr(),
                    "{}: {}: invalid signal specification",
                    context.command_name,
                    possible_sigspec
                )?;
                return Ok(ExecutionResult::general_error());
            }
        } else if pid_or_job_spec.is_none() {
            pid_or_job_spec = Some(arg);
        } else {
            writeln!(
                context.stderr(),
                "{}: too many jobs or processes specified",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }
    }

    if command.list_signals {
        return print_signals(&context, command.args.as_ref());
    } else {
        let Some(pid_or_job_spec) = pid_or_job_spec else {
            writeln!(context.stderr(), "{}: invalid usage", context.command_name)?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        };

        if pid_or_job_spec.starts_with('%') {
            // It's a job spec.
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(pid_or_job_spec) {
                job.kill(trap_signal)?;
            } else {
                writeln!(
                    context.stderr(),
                    "{}: {}: no such job",
                    context.command_name,
                    pid_or_job_spec
                )?;
                return Ok(ExecutionResult::general_error());
            }
        } else {
            let pid = brush_core::int_utils::parse(pid_or_job_spec.as_str(), 10)?;

            // It's a pid.
            sys::signal::kill_process(pid, trap_signal)?;
        }
    }
    Ok(ExecutionResult::success())
}
