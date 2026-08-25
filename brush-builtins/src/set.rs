//! The `set` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(SetCommand);

use brush_core::{ExecutionExitCode, ExecutionResult, variables};
use itertools::Itertools;
use std::collections::HashMap;
use std::io::Write;

pub(super) fn display_all(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<(), brush_core::Error> {
    // Display variables.
    for (name, var) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if !var.is_enumerable() {
            continue;
        }

        // TODO(set): For now, skip all dynamic variables. The current behavior
        // of bash is not quite clear. We've empirically found that some
        // special variables don't get displayed until they're observed
        // at least once.
        if matches!(var.value(), variables::ShellValue::Dynamic { .. }) {
            continue;
        }

        // Skip variables that have been declared but are unset.
        if !var.value().is_set() {
            continue;
        }

        writeln!(
            context.stdout(),
            "{name}={}",
            var.value()
                .format(variables::FormatStyle::Basic, context.shell)?,
        )?;
    }

    // Display functions... unless we're in posix compliance mode.
    if !context.shell.options().posix_mode {
        for (_name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
            writeln!(context.stdout(), "{}", registration.definition())?;
        }
    }

    Ok(())
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
#[expect(
    clippy::too_many_lines,
    reason = "long option-processing chain mirroring bash `set` semantics"
)]
#[expect(
    clippy::useless_let_if_seq,
    reason = "flag-accumulation pattern kept close to upstream shape"
)]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &SetCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();

    let mut saw_option = false;

    if let Some(value) = command.print_commands_and_arguments.to_bool() {
        context.shell.options_mut().print_commands_and_arguments = value;
        saw_option = true;
    }

    if let Some(value) = command.export_variables_on_modification.to_bool() {
        context.shell.options_mut().export_variables_on_modification = value;
        saw_option = true;
    }

    if let Some(value) = command.notify_job_termination_immediately.to_bool() {
        context
            .shell
            .options_mut()
            .notify_job_termination_immediately = value;
        saw_option = true;
    }

    if let Some(value) = command.exit_on_nonzero_command_exit.to_bool() {
        context.shell.options_mut().exit_on_nonzero_command_exit = value;
        saw_option = true;
    }

    if let Some(value) = command.disable_filename_globbing.to_bool() {
        context.shell.options_mut().disable_filename_globbing = value;
        saw_option = true;
    }

    if let Some(value) = command.remember_command_locations.to_bool() {
        context.shell.options_mut().remember_command_locations = value;
        saw_option = true;
    }

    if let Some(value) = command.place_all_assignment_args_in_command_env.to_bool() {
        context
            .shell
            .options_mut()
            .place_all_assignment_args_in_command_env = value;
        saw_option = true;
    }

    if let Some(value) = command.enable_job_control.to_bool() {
        context.shell.options_mut().enable_job_control = value;
        saw_option = true;
    }

    if let Some(value) = command.do_not_execute_commands.to_bool() {
        context.shell.options_mut().do_not_execute_commands = value;
        saw_option = true;
    }

    if let Some(value) = command.real_effective_uid_mismatch.to_bool() {
        context.shell.options_mut().real_effective_uid_mismatch = value;
        saw_option = true;
    }

    if let Some(value) = command.exit_after_one_command.to_bool() {
        context.shell.options_mut().exit_after_one_command = value;
        saw_option = true;
    }

    if let Some(value) = command.treat_unset_variables_as_error.to_bool() {
        context.shell.options_mut().treat_unset_variables_as_error = value;
        saw_option = true;
    }

    if let Some(value) = command.print_shell_input_lines.to_bool() {
        context.shell.options_mut().print_shell_input_lines = value;
        saw_option = true;
    }

    if let Some(value) = command.print_commands_and_arguments.to_bool() {
        context.shell.options_mut().print_commands_and_arguments = value;
        saw_option = true;
    }

    if let Some(value) = command.perform_brace_expansion.to_bool() {
        context.shell.options_mut().perform_brace_expansion = value;
        saw_option = true;
    }

    if let Some(value) = command
        .disallow_overwriting_regular_files_via_output_redirection
        .to_bool()
    {
        context
            .shell
            .options_mut()
            .disallow_overwriting_regular_files_via_output_redirection = value;
        saw_option = true;
    }

    if let Some(value) = command.shell_functions_inherit_err_trap.to_bool() {
        context.shell.options_mut().shell_functions_inherit_err_trap = value;
        saw_option = true;
    }

    if let Some(value) = command.enable_bang_style_history_substitution.to_bool() {
        context
            .shell
            .options_mut()
            .enable_bang_style_history_substitution = value;
        saw_option = true;
    }

    if let Some(value) = command.do_not_resolve_symlinks_when_changing_dir.to_bool() {
        context
            .shell
            .options_mut()
            .do_not_resolve_symlinks_when_changing_dir = value;
        saw_option = true;
    }

    if let Some(value) = command
        .shell_functions_inherit_debug_and_return_traps
        .to_bool()
    {
        context
            .shell
            .options_mut()
            .shell_functions_inherit_debug_and_return_traps = value;
        saw_option = true;
    }

    let mut named_options: HashMap<String, bool> = HashMap::new();
    if let Some(option_names) = &command.set_option.disable {
        saw_option = true;
        if option_names.is_empty() {
            for option in
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                    .iter()
                    .sorted_by_key(|option| option.name)
            {
                let option_value = option.definition.get(context.shell.options());
                let option_value_str = if option_value { "-o" } else { "+o" };
                writeln!(context.stdout(), "set {option_value_str} {}", option.name)?;
            }
        } else {
            for option_name in option_names {
                named_options.insert(option_name.to_owned(), false);
            }
        }
    }
    if let Some(option_names) = &command.set_option.enable {
        saw_option = true;
        if option_names.is_empty() {
            for option in
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                    .iter()
                    .sorted_by_key(|option| option.name)
            {
                let option_value = option.definition.get(context.shell.options());
                let option_value_str = if option_value { "on" } else { "off" };
                writeln!(context.stdout(), "{:15}\t{option_value_str}", option.name)?;
            }
        } else {
            for option_name in option_names {
                named_options.insert(option_name.to_owned(), true);
            }
        }
    }

    for (option_name, value) in named_options {
        if let Some(option_def) =
            brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                .get(option_name.as_str())
        {
            option_def.set(context.shell.options_mut(), value);
        } else {
            result = ExecutionExitCode::InvalidUsage.into();
        }
    }

    let args = context.shell.current_shell_args_mut();

    let skip = match command.positional_args.first() {
        Some(x) if x == "-" => {
            if command.positional_args.len() > 1 {
                args.clear();
            }
            1
        }
        Some(x) if x == "--" => {
            args.clear();
            1
        }
        Some(_) => {
            args.clear();
            0
        }
        None => 0,
    };

    for arg in command.positional_args.iter().skip(skip) {
        args.push(arg.to_owned());
    }

    saw_option = saw_option || !command.positional_args.is_empty();

    // If we *still* haven't seen any options, then we need to display all variables and
    // functions.
    if !saw_option {
        display_all(&context)?;
    }

    Ok(result)
}
