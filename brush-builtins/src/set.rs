use std::collections::HashMap;
use std::io::Write;

use itertools::Itertools;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

/// Sentinel bound to a bare `-o`/`+o` (a list-all request). `default_missing` injects
/// it for an occurrence without a value; it is not a real option name.
const BARE_OPTION: &str = "\u{0}";

/// Manage set-based shell options.
#[derive(winnow_args::Args)]
#[arg(
    plus_options,
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct SetCommand {
    /// Export variables on modification.
    #[arg(short = 'a', plus = 'a')]
    export_variables_on_modification: Option<bool>,

    /// Notify job termination immediately.
    #[arg(short = 'b', plus = 'b')]
    notify_job_termination_immediately: Option<bool>,

    /// Exit on nonzero command exit.
    #[arg(short = 'e', plus = 'e')]
    exit_on_nonzero_command_exit: Option<bool>,

    /// Disable filename globbing.
    #[arg(short = 'f', plus = 'f')]
    disable_filename_globbing: Option<bool>,

    /// Remember command locations.
    #[arg(short = 'h', plus = 'h')]
    remember_command_locations: Option<bool>,

    /// Place all assignment args in command environment.
    #[arg(short = 'k', plus = 'k')]
    place_all_assignment_args_in_command_env: Option<bool>,

    /// Enable job control.
    #[arg(short = 'm', plus = 'm')]
    enable_job_control: Option<bool>,

    /// Do not execute commands.
    #[arg(short = 'n', plus = 'n')]
    do_not_execute_commands: Option<bool>,

    /// Real effective UID mismatch.
    #[arg(short = 'p', plus = 'p')]
    real_effective_uid_mismatch: Option<bool>,

    /// Exit after one command.
    #[arg(short = 't', plus = 't')]
    exit_after_one_command: Option<bool>,

    /// Treat unset variables as error.
    #[arg(short = 'u', plus = 'u')]
    treat_unset_variables_as_error: Option<bool>,

    /// Print shell input lines.
    #[arg(short = 'v', plus = 'v')]
    print_shell_input_lines: Option<bool>,

    /// Print commands and arguments.
    #[arg(short = 'x', plus = 'x')]
    print_commands_and_arguments: Option<bool>,

    /// Perform brace expansion.
    #[arg(short = 'B', plus = 'B')]
    perform_brace_expansion: Option<bool>,

    /// Disallow overwriting regular files via output redirection.
    #[arg(short = 'C', plus = 'C')]
    disallow_overwriting_regular_files_via_output_redirection: Option<bool>,

    /// Shell functions inherit ERR trap.
    #[arg(short = 'E', plus = 'E')]
    shell_functions_inherit_err_trap: Option<bool>,

    /// Enable bang style history substitution.
    #[arg(short = 'H', plus = 'H')]
    enable_bang_style_history_substitution: Option<bool>,

    /// Do not resolve symlinks when changing dir.
    #[arg(short = 'P', plus = 'P')]
    do_not_resolve_symlinks_when_changing_dir: Option<bool>,

    /// Shell functions inherit DEBUG and RETURN traps.
    #[arg(short = 'T', plus = 'T')]
    shell_functions_inherit_debug_and_return_traps: Option<bool>,

    /// Set the named option (`-o NAME`); alone, list the options.
    #[arg(short = 'o', value_name = "option-name", default_missing = "\u{0}")]
    enable: Vec<String>,

    /// Unset the named option (`+o NAME`); alone, list them as commands.
    #[arg(plus = 'o', value_name = "option-name", default_missing = "\u{0}")]
    disable: Vec<String>,

    /// Positional parameters, a leading `-` or `--` kept.
    #[arg(positional, value_name = "arg", double_dash = "preserve", stop_flags)]
    positional_args: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(SetCommand, trailing_args = positional_args);

impl builtins::Command for SetCommand {
    type Error = brush_core::Error;

    #[expect(clippy::too_many_lines)]
    #[allow(clippy::useless_let_if_seq)]
    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        let mut saw_option = false;

        if let Some(value) = self.print_commands_and_arguments {
            context.shell.options_mut().print_commands_and_arguments = value;
            saw_option = true;
        }

        if let Some(value) = self.export_variables_on_modification {
            context.shell.options_mut().export_variables_on_modification = value;
            saw_option = true;
        }

        if let Some(value) = self.notify_job_termination_immediately {
            context
                .shell
                .options_mut()
                .notify_job_termination_immediately = value;
            saw_option = true;
        }

        if let Some(value) = self.exit_on_nonzero_command_exit {
            context.shell.options_mut().exit_on_nonzero_command_exit = value;
            saw_option = true;
        }

        if let Some(value) = self.disable_filename_globbing {
            context.shell.options_mut().disable_filename_globbing = value;
            saw_option = true;
        }

        if let Some(value) = self.remember_command_locations {
            context.shell.options_mut().remember_command_locations = value;
            saw_option = true;
        }

        if let Some(value) = self.place_all_assignment_args_in_command_env {
            context
                .shell
                .options_mut()
                .place_all_assignment_args_in_command_env = value;
            saw_option = true;
        }

        if let Some(value) = self.enable_job_control {
            context.shell.options_mut().enable_job_control = value;
            saw_option = true;
        }

        if let Some(value) = self.do_not_execute_commands {
            context.shell.options_mut().do_not_execute_commands = value;
            saw_option = true;
        }

        if let Some(value) = self.real_effective_uid_mismatch {
            context.shell.options_mut().real_effective_uid_mismatch = value;
            saw_option = true;
        }

        if let Some(value) = self.exit_after_one_command {
            context.shell.options_mut().exit_after_one_command = value;
            saw_option = true;
        }

        if let Some(value) = self.treat_unset_variables_as_error {
            context.shell.options_mut().treat_unset_variables_as_error = value;
            saw_option = true;
        }

        if let Some(value) = self.print_shell_input_lines {
            context.shell.options_mut().print_shell_input_lines = value;
            saw_option = true;
        }

        if let Some(value) = self.print_commands_and_arguments {
            context.shell.options_mut().print_commands_and_arguments = value;
            saw_option = true;
        }

        if let Some(value) = self.perform_brace_expansion {
            context.shell.options_mut().perform_brace_expansion = value;
            saw_option = true;
        }

        if let Some(value) = self.disallow_overwriting_regular_files_via_output_redirection {
            context
                .shell
                .options_mut()
                .disallow_overwriting_regular_files_via_output_redirection = value;
            saw_option = true;
        }

        if let Some(value) = self.shell_functions_inherit_err_trap {
            context.shell.options_mut().shell_functions_inherit_err_trap = value;
            saw_option = true;
        }

        if let Some(value) = self.enable_bang_style_history_substitution {
            context
                .shell
                .options_mut()
                .enable_bang_style_history_substitution = value;
            saw_option = true;
        }

        if let Some(value) = self.do_not_resolve_symlinks_when_changing_dir {
            context
                .shell
                .options_mut()
                .do_not_resolve_symlinks_when_changing_dir = value;
            saw_option = true;
        }

        if let Some(value) = self.shell_functions_inherit_debug_and_return_traps {
            context
                .shell
                .options_mut()
                .shell_functions_inherit_debug_and_return_traps = value;
            saw_option = true;
        }

        let mut named_options: HashMap<String, bool> = HashMap::new();
        if !self.disable.is_empty() {
            let option_names = &self.disable;
            saw_option = true;
            if option_names.iter().all(|name| name == BARE_OPTION) {
                for option in brush_core::namedoptions::options(
                    brush_core::namedoptions::ShellOptionKind::SetO,
                )
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
        if !self.enable.is_empty() {
            let option_names = &self.enable;
            saw_option = true;
            if option_names.iter().all(|name| name == BARE_OPTION) {
                for option in brush_core::namedoptions::options(
                    brush_core::namedoptions::ShellOptionKind::SetO,
                )
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

        let skip = match self.positional_args.first() {
            Some(x) if x == "-" => {
                if self.positional_args.len() > 1 {
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

        for arg in self.positional_args.iter().skip(skip) {
            args.push(arg.to_owned());
        }

        saw_option = saw_option || !self.positional_args.is_empty();

        // If we *still* haven't seen any options, then we need to display all variables and
        // functions.
        if !saw_option {
            display_all(&context)?;
        }

        Ok(result)
    }
}

fn display_all(
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
