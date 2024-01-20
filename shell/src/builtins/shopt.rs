use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct ShoptCommand {
    #[arg(short = 'o')]
    set_o_names_only: bool,

    #[arg(short = 'p')]
    print: bool,

    #[arg(short = 'q')]
    quiet: bool,

    #[arg(short = 's')]
    set: bool,

    #[arg(short = 'u')]
    unset: bool,

    options: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for ShoptCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.set && self.unset {
            log::error!("cannot set and unset shell options simultaneously");
            return Ok(BuiltinExitCode::InvalidUsage);
        }

        if self.options.is_empty() {
            if self.quiet {
                return Ok(BuiltinExitCode::Success);
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
                let option_value = (option_definition.getter)(context.shell);
                if self.set && !option_value {
                    continue;
                }
                if self.unset && option_value {
                    continue;
                }

                if self.print {
                    let option_value_str = if option_value { "-s" } else { "-u" };
                    println!("shopt {option_value_str} {option_name}");
                } else {
                    let option_value_str = if option_value { "on" } else { "off" };
                    println!("{option_name:15} {option_value_str}");
                }
            }

            Ok(BuiltinExitCode::Success)
        } else {
            let mut return_value = BuiltinExitCode::Success;

            // Enumerate only the specified options.
            for option_name in &self.options {
                let option_definition = if self.set_o_names_only {
                    crate::namedoptions::SET_O_OPTIONS.get(option_name.as_str())
                } else {
                    crate::namedoptions::SHOPT_OPTIONS.get(option_name.as_str())
                };

                if let Some(option_definition) = option_definition {
                    if self.set {
                        (option_definition.setter)(context.shell, true);
                    } else if self.unset {
                        (option_definition.setter)(context.shell, false);
                    } else {
                        let option_value = (option_definition.getter)(context.shell);
                        if !option_value {
                            return_value = BuiltinExitCode::Custom(1);
                        }

                        if !self.quiet {
                            if self.print {
                                let option_value_str = if option_value { "-s" } else { "-u" };
                                println!("shopt {option_value_str} {option_name}");
                            } else {
                                let option_value_str = if option_value { "on" } else { "off" };
                                println!("{option_name:15} {option_value_str}");
                            }
                        }
                    }
                } else {
                    eprintln!(
                        "{}: {}: invalid shell option name",
                        context.builtin_name, option_name
                    );
                    return_value = BuiltinExitCode::Custom(1);
                }
            }

            Ok(return_value)
        }
    }
}
