//! The `shopt` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ShoptCommand);

use brush_core::{ExecutionExitCode, ExecutionResult};
use itertools::Itertools;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ShoptCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.set && command.unset {
        writeln!(
            context.stderr(),
            "cannot set and unset shell options simultaneously"
        )?;
        return Ok(ExecutionExitCode::InvalidUsage.into());
    }

    if command.options.is_empty() {
        if command.quiet {
            return Ok(ExecutionResult::success());
        }

        // Enumerate all options of the selected type.
        let options = if command.set_o_names_only {
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
            if command.set && !option_value {
                continue;
            }
            if command.unset && option_value {
                continue;
            }

            if command.print {
                if command.set_o_names_only {
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
        for option_name in &command.options {
            let option_definition = if command.set_o_names_only {
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                    .get(option_name.as_str())
            } else {
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::Shopt)
                    .get(option_name.as_str())
            };

            if let Some(option_definition) = option_definition {
                if command.set {
                    option_definition.set(context.shell.options_mut(), true);
                } else if command.unset {
                    option_definition.set(context.shell.options_mut(), false);
                } else {
                    let option_value = option_definition.get(context.shell.options());
                    if !option_value {
                        return_value = ExecutionResult::general_error();
                    }

                    if !command.quiet {
                        if command.print {
                            if command.set_o_names_only {
                                let option_value_str = if option_value { "-o" } else { "+o" };
                                writeln!(context.stdout(), "set {option_value_str} {option_name}")?;
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
