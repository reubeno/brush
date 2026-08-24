use bpaf::Parser;
use std::{ffi::OsStr, io::Write};

use brush_core::traps::TrapSignal;
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, sys};

/// Signal a job or process.
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    signal_name: Option<String>,

    /// Number of the signal to send.
    signal_number: Option<usize>,

    /// List known signal names.
    list_signals: bool,

    /// Remaining arguments; may contain pids/job specs as well as `-sigspec`
    /// style options, whose interpretation depends on whether `-l` is present.
    args: Vec<String>,
}

impl builtins::Command for KillCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let signal_name = bpaf::short('s')
            .help("Name of the signal to send.")
            .argument::<String>("SIG_NAME")
            .optional();
        let signal_number = bpaf::short('n')
            .help("Number of the signal to send.")
            .argument::<usize>("SIG_NUM")
            .optional();
        let list_l = bpaf::short('l')
            .help("List known signal names.")
            .req_flag(())
            .map(|(): ()| Some(true));
        let list_capital_l = bpaf::short('L').req_flag(()).map(|(): ()| Some(true));
        let list_signals = bpaf::construct!([list_l, list_capital_l])
            .fallback(None)
            .map(|v: Option<bool>| v.is_some());
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(KillCommand {
            signal_name,
            signal_number,
            list_signals,
            args,
        })
    }

    fn about() -> &'static str {
        "Signal a job or process."
    }

    fn synopsis() -> &'static str {
        "[-s SIG_NAME | -n SIG_NUM | -lL] [PID_OR_JOB_SPEC]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn value_taking_short_options() -> &'static str {
        "sn"
    }

    /// N.B. Overrides the default [`builtins::Command::new`] because `-sigspec`
    /// style options (e.g., `kill -9` or `kill -TERM`) look like flags but must
    /// be captured verbatim alongside pids and job specs so that `execute` can
    /// interpret them.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut options = Vec::new();
        let mut trailing = Vec::new();

        // N.B. The first argument is the command name itself.
        let mut iter = args.into_iter().skip(1);
        let mut pending_value = false;
        while let Some(arg) = iter.next() {
            if pending_value {
                // This token is the value of a preceding value-taking option.
                options.push(arg);
                pending_value = false;
                continue;
            }

            if arg == "--" {
                trailing.extend(iter);
                break;
            }

            if !arg.starts_with('-') || arg == "-" {
                // An operand; everything from here on is captured verbatim.
                trailing.push(arg);
                trailing.extend(iter);
                break;
            }

            if arg.starts_with('-')
                && !arg.starts_with("--")
                && arg.chars().nth(1).is_none_or(|c| !"snlL".contains(c))
            {
                // A `-sigspec` style token (e.g., `-9` or `-TERM`).
                trailing.push(arg);
                continue;
            }

            if let Some(group) = arg.strip_prefix('-').filter(|g| !g.starts_with('-')) {
                let chars: Vec<char> = group.chars().collect();
                for (j, c) in chars.iter().enumerate() {
                    match c {
                        's' | 'n' => {
                            pending_value = j == chars.len() - 1;
                            break;
                        }
                        'l' | 'L' => {}
                        _ => break,
                    }
                }
            }

            options.push(arg);
        }

        let mut command = run_bpaf_parser::<Self>(&options)?;
        command.set_trailing_args(trailing);

        Ok(command)
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
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
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }
        }

        // Try parsing the signal number (if specified).
        if let Some(signal_number) = &self.signal_number {
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
        for arg in &self.args {
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

        if self.list_signals {
            return print_signals(&context, self.args.as_ref());
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
}

fn print_signals(
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

fn run_bpaf_parser<T: builtins::Command>(
    args: &[String],
) -> Result<T, builtins::BuiltinArgParseError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_bpaf_failure)
}

fn render_bpaf_failure(failure: bpaf::ParseFailure) -> builtins::BuiltinArgParseError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => builtins::BuiltinArgParseError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => builtins::BuiltinArgParseError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => builtins::BuiltinArgParseError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::builtins::Command as _;

    #[test]
    fn parse_s_with_name() -> anyhow::Result<()> {
        let cmd = KillCommand::new(["kill", "-s", "TERM", "123"].iter().map(|s| s.to_string()))?;
        assert_eq!(cmd.signal_name.as_deref(), Some("TERM"));
        assert_eq!(cmd.args, ["123"]);
        Ok(())
    }

    #[test]
    fn parse_dash_sigspec() -> anyhow::Result<()> {
        let cmd = KillCommand::new(["kill", "-USR1", "123"].iter().map(|s| s.to_string()))?;
        assert!(cmd.signal_name.is_none());
        assert_eq!(cmd.args, ["-USR1", "123"]);
        Ok(())
    }
}
