use std::io::Write;

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

const ID_SIGNAL_NAME: &str = "signal_name";
const ID_SIGNAL_NUMBER: &str = "signal_number";
const ID_LIST_SIGNALS: &str = "list_signals";

impl builtins::SpecCommand for KillCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        // N.B. `-L` is a hidden alias for `-l`.
        spec.arg(
            ID_SIGNAL_NAME,
            &['s'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("SIG_NAME"),
            "Name of the signal to send.",
        )
        .arg(
            ID_SIGNAL_NUMBER,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("SIG_NUM"),
            "Number of the signal to send.",
        )
        .arg(
            ID_LIST_SIGNALS,
            &['l', 'L'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "List known signal names.",
        )
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let signal_number = match matches.value(ID_SIGNAL_NUMBER) {
            Some(v) => Some(
                v.parse::<usize>()
                    .map_err(|_| builtins::BuiltinArgParseError {
                        message: format!("invalid signal number: {v}"),
                        help_request: false,
                    })?,
            ),
            None => None,
        };

        Ok(Self {
            signal_name: matches.value(ID_SIGNAL_NAME).map(str::to_string),
            signal_number,
            list_signals: matches.flag(ID_LIST_SIGNALS),
            args: matches.trailing().to_vec(),
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

    /// N.B. Overrides the default [`builtins::SpecCommand::new`] because `-sigspec`
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

        let spec = Self::declare(builtins::argmodel::CommandSpecBuilder::new()).build();
        let mut matches = builtins::argmodel::backend().parse(&spec, "", &options)?;
        matches.set_trailing(trailing);

        Self::from_matches(&mut matches)
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

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::builtins::SpecCommand as _;

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
