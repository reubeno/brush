use brush_core::writeln_async;
use itertools::Itertools;

use brush_core::{ExecutionResult, builtins};

/// Manage shopt-style options.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
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
    #[arg(positional)]
    options: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(ShoptCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ShoptCommand {
    type Error = brush_core::Error;

    #[allow(clippy::too_many_lines)]
    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.set && self.unset {
            writeln_async!(
                context.stderr_async(),
                "{}: cannot set and unset shell options simultaneously",
                context.command_name
            )
            .await?;
            return Ok(ExecutionResult::general_error());
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
                        writeln_async!(
                            context.stdout_async(),
                            "set {option_value_str} {}",
                            option.name
                        )
                        .await?;
                    } else {
                        let option_value_str = if option_value { "-s" } else { "-u" };
                        writeln_async!(
                            context.stdout_async(),
                            "shopt {option_value_str} {}",
                            option.name
                        )
                        .await?;
                    }
                } else {
                    let option_value_str = if option_value { "on" } else { "off" };
                    writeln_async!(
                        context.stdout_async(),
                        "{:20}\t{option_value_str}",
                        option.name
                    )
                    .await?;
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
                                    writeln_async!(
                                        context.stdout_async(),
                                        "set {option_value_str} {option_name}"
                                    )
                                    .await?;
                                } else {
                                    let option_value_str = if option_value { "-s" } else { "-u" };
                                    writeln_async!(
                                        context.stdout_async(),
                                        "shopt {option_value_str} {option_name}"
                                    )
                                    .await?;
                                }
                            } else {
                                let option_value_str = if option_value { "on" } else { "off" };
                                writeln_async!(
                                    context.stdout_async(),
                                    "{option_name:20}\t{option_value_str}"
                                )
                                .await?;
                            }
                        }
                    }
                } else {
                    writeln_async!(
                        context.stderr_async(),
                        "{}: {}: invalid shell option name",
                        context.command_name,
                        option_name
                    )
                    .await?;
                    return_value = ExecutionResult::general_error();
                }
            }

            Ok(return_value)
        }
    }
}
