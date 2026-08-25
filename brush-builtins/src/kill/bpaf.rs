//! `kill` builtin: `KillCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use std::io::Write;
use brush_core::traps::TrapSignal;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::{ExecutionResult, builtins};

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

fn run_bpaf_parser<T: crate::args::BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    crate::args::run_parser::<T>(args)
}

fn render_bpaf_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
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

/// Signal a job or process.
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    pub(super) signal_name: Option<String>,

    /// Number of the signal to send.
    pub(super) signal_number: Option<usize>,

    /// List known signal names.
    pub(super) list_signals: bool,

    /// Remaining arguments; may contain pids/job specs as well as `-sigspec`
    /// style options, whose interpretation depends on whether `-l` is present.
    pub(super) args: Vec<String>,
}

impl crate::args::BpafArgs for KillCommand {
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
        // N.B. `-L` is a hidden alias for `-l`, matching clap's short_alias.
        let list_signals = bpaf::short('l')
            .short('L')
            .help("List known signal names.")
            .req_flag(())
            .map(|(): ()| Some(true))
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
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

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
}

impl FromArgs for KillCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for KillCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
