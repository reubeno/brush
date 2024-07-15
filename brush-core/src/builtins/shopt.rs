use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::{builtins, commands};

/// Manage shopt-style options.
#[derive(Parser)]
pub(crate) struct ShoptCommand {
    /// Manage set -o options.
    #[arg(short = 'o')]
    set_o_names_only: bool,

    /// Print options' current values.
    #[arg(short = 'p')]
    print: bool,

    /// Suppress typical output.
    #[arg(short = 'q')]
    quiet: bool,

    /// Set the specified options.
    #[arg(short = 's')]
    set: bool,

    /// Unset the specified options.
    #[arg(short = 'u')]
    unset: bool,

    /// Names of options to operate on.
    options: Vec<String>,
}

#[async_trait::async_trait]
impl builtins::Command for ShoptCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.set && self.unset {
            writeln!(
                context.stderr(),
                "cannot set and unset shell options simultaneously"
            )?;
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        if self.options.is_empty() {
            if self.quiet {
                return Ok(builtins::ExitCode::Success);
            }

            // Enumerate all options of the selected type.
            let options = if self.set_o_names_only {
                crate::namedoptions::SET_O_OPTIONS
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
            } else {
                crate::namedoptions::SHOPT_OPTIONS
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
            };

            for (option_name, option_definition) in options {
                let option_value = (option_definition.getter)(&context.shell.options);
                if self.set && !option_value {
                    continue;
                }
                if self.unset && option_value {
                    continue;
                }

                if self.print {
                    if self.set_o_names_only {
                        let option_value_str = if option_value { "-o" } else { "+o" };
                        writeln!(context.stdout(), "set {option_value_str} {option_name}")?;
                    } else {
                        let option_value_str = if option_value { "-s" } else { "-u" };
                        writeln!(context.stdout(), "shopt {option_value_str} {option_name}")?;
                    }
                } else {
                    let option_value_str = if option_value { "on" } else { "off" };
                    writeln!(context.stdout(), "{option_name:15}\t{option_value_str}")?;
                }
            }

            Ok(builtins::ExitCode::Success)
        } else {
            let mut return_value = builtins::ExitCode::Success;

            // Enumerate only the specified options.
            for option_name in &self.options {
                let option_definition = if self.set_o_names_only {
                    crate::namedoptions::SET_O_OPTIONS.get(option_name.as_str())
                } else {
                    crate::namedoptions::SHOPT_OPTIONS.get(option_name.as_str())
                };

                if let Some(option_definition) = option_definition {
                    if self.set {
                        (option_definition.setter)(&mut context.shell.options, true);
                    } else if self.unset {
                        (option_definition.setter)(&mut context.shell.options, false);
                    } else {
                        let option_value = (option_definition.getter)(&context.shell.options);
                        if !option_value {
                            return_value = builtins::ExitCode::Custom(1);
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
                                writeln!(context.stdout(), "{option_name:15}\t{option_value_str}")?;
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
                    return_value = builtins::ExitCode::Custom(1);
                }
            }

            Ok(return_value)
        }
    }
}
