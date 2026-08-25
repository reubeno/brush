use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

const ID_SET_O_NAMES_ONLY: &str = "set_o_names_only";
const ID_PRINT: &str = "print";
const ID_QUIET: &str = "quiet";
const ID_SET: &str = "set";
const ID_UNSET: &str = "unset";
const ID_OPTIONS: &str = "options";

/// Manage shopt-style options.
pub(crate) struct ShoptCommand {
    set_o_names_only: bool,
    print: bool,
    quiet: bool,
    set: bool,
    unset: bool,
    options: Vec<String>,
}

impl builtins::SpecCommand for ShoptCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_SET_O_NAMES_ONLY,
            &['o'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Manage set -o options.",
        )
        .arg(
            ID_PRINT,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Print options' current values.",
        )
        .arg(
            ID_QUIET,
            &['q'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Suppress typical output.",
        )
        .arg(
            ID_SET,
            &['s'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Set the specified options.",
        )
        .arg(
            ID_UNSET,
            &['u'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Unset the specified options.",
        )
        .positional_many(ID_OPTIONS, "OPTIONS")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            set_o_names_only: matches.flag(ID_SET_O_NAMES_ONLY),
            print: matches.flag(ID_PRINT),
            quiet: matches.flag(ID_QUIET),
            set: matches.flag(ID_SET),
            unset: matches.flag(ID_UNSET),
            options: matches.values(ID_OPTIONS).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Manage shopt-style options."
    }

    fn synopsis() -> &'static str {
        "[-opqsu] [OPTIONS]..."
    }

    #[allow(clippy::too_many_lines)]
    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.set && self.unset {
            writeln!(
                context.stderr(),
                "cannot set and unset shell options simultaneously"
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        if self.options.is_empty() {
            if self.quiet {
                return Ok(ExecutionResult::success());
            }

            // Enumerate all options of the selected type.
            let options = if self.set_o_names_only {
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                    .iter()
                    .sorted_by_key(|opt| opt.name)
            } else {
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::Shopt)
                    .iter()
                    .sorted_by_key(|opt| opt.name)
            };

            for option in options {
                let option_value = option.definition.get(context.shell.options());
                if self.set && !option_value {
                    continue;
                }
                if self.unset && option_value {
                    continue;
                }

                if self.print {
                    if self.set_o_names_only {
                        let option_value_str = if option_value { "-o" } else { "+o" };
                        writeln!(context.stdout(), "set {option_value_str} {}", option.name)?;
                    } else {
                        let option_value_str = if option_value { "-s" } else { "-u" };
                        writeln!(context.stdout(), "shopt {option_value_str} {}", option.name)?;
                    }
                } else {
                    let option_value_str = if option_value { "on" } else { "off" };
                    writeln!(context.stdout(), "{:20}\t{option_value_str}", option.name)?;
                }
            }

            Ok(ExecutionResult::success())
        } else {
            let mut return_value = ExecutionResult::success();

            // Enumerate only the specified options.
            for option_name in &self.options {
                let option_definition = if self.set_o_names_only {
                    brush_core::namedoptions::options(
                        brush_core::namedoptions::ShellOptionKind::SetO,
                    )
                    .get(option_name.as_str())
                } else {
                    brush_core::namedoptions::options(
                        brush_core::namedoptions::ShellOptionKind::Shopt,
                    )
                    .get(option_name.as_str())
                };

                if let Some(option_definition) = option_definition {
                    if self.set {
                        option_definition.set(context.shell.options_mut(), true);
                    } else if self.unset {
                        option_definition.set(context.shell.options_mut(), false);
                    } else {
                        let option_value = option_definition.get(context.shell.options());
                        if !option_value {
                            return_value = ExecutionResult::general_error();
                        }

                        if !self.quiet {
                            if self.print {
                                if self.set_o_names_only {
                                    let option_value_str = if option_value { "-o" } else { "+o" };
                                    writeln!(
                                        context.stdout(),
                                        "set {option_value_str} {option_name}"
                                    )?;
                                } else {
                                    let option_value_str = if option_value { "-s" } else { "-u" };
                                    writeln!(
                                        context.stdout(),
                                        "shopt {option_value_str} {option_name}"
                                    )?;
                                }
                            } else {
                                let option_value_str = if option_value { "on" } else { "off" };
                                writeln!(context.stdout(), "{option_name:20}\t{option_value_str}")?;
                            }
                        }
                    }
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {}: invalid shell option name",
                        context.command_name,
                        option_name
                    )?;
                    return_value = ExecutionResult::general_error();
                }
            }

            Ok(return_value)
        }
    }
}
