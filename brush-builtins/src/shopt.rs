use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

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

impl builtins::Command for ShoptCommand {
    type Error = brush_core::Error;

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

        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        if self.options.is_empty() {
            if self.quiet {
                return Ok(ExecutionResult::success());
            }

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
                        writeln!(output, "set {option_value_str} {}", option.name)?;
                    } else {
                        let option_value_str = if option_value { "-s" } else { "-u" };
                        writeln!(output, "shopt {option_value_str} {}", option.name)?;
                    }
                } else {
                    let option_value_str = if option_value { "on" } else { "off" };
                    writeln!(output, "{:20}\t{option_value_str}", option.name)?;
                }
            }
        } else {
            let mut return_value = ExecutionResult::success();

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
                                    writeln!(output, "set {option_value_str} {option_name}")?;
                                } else {
                                    let option_value_str = if option_value { "-s" } else { "-u" };
                                    writeln!(output, "shopt {option_value_str} {option_name}")?;
                                }
                            } else {
                                let option_value_str = if option_value { "on" } else { "off" };
                                writeln!(output, "{option_name:20}\t{option_value_str}")?;
                            }
                        }
                    }
                } else {
                    writeln!(
                        stderr_output,
                        "{}: {}: invalid shell option name",
                        context.command_name, option_name
                    )?;
                    return_value = ExecutionResult::general_error();
                }
            }

            if !stderr_output.is_empty() {
                context.stderr().write_all(&stderr_output)?;
                context.stderr().flush()?;
            }

            if !output.is_empty() {
                if let Some(mut stdout) = context.stdout_async() {
                    stdout.write_all(&output).await?;
                    stdout.flush().await?;
                } else {
                    context.stdout().write_all(&output)?;
                    context.stdout().flush()?;
                }
            }

            return Ok(return_value);
        }

        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        Ok(ExecutionResult::success())
    }
}
