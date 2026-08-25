use itertools::Itertools;
use std::io::Write;

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues, PositionalSpec};
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

static SHOPT_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(ID_SET_O_NAMES_ONLY, &['o'], &[], "Manage set -o options."),
        ArgSpec::flag(ID_PRINT, &['p'], &[], "Print options' current values."),
        ArgSpec::flag(ID_QUIET, &['q'], &[], "Suppress typical output."),
        ArgSpec::flag(ID_SET, &['s'], &[], "Set the specified options."),
        ArgSpec::flag(ID_UNSET, &['u'], &[], "Unset the specified options."),
    ],
    positionals: &[PositionalSpec::many(ID_OPTIONS, "OPTIONS")],
};

impl builtins::SpecCommand for ShoptCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &SHOPT_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            set_o_names_only: values.flag(ID_SET_O_NAMES_ONLY),
            print: values.flag(ID_PRINT),
            quiet: values.flag(ID_QUIET),
            set: values.flag(ID_SET),
            unset: values.flag(ID_UNSET),
            options: values.positional_values(ID_OPTIONS).to_vec(),
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
