use std::collections::HashMap;

use clap::Parser;

use crate::{builtin, commands, error, namedoptions};

builtin::minus_or_plus_flag_arg!(
    ExportVariablesOnModification,
    'a',
    "Export variables on modification"
);
builtin::minus_or_plus_flag_arg!(
    NotfyJobTerminationImmediately,
    'b',
    "Notify job termination immediately"
);
builtin::minus_or_plus_flag_arg!(
    ExitOnNonzeroCommandExit,
    'e',
    "Exit on nonzero command exit"
);
builtin::minus_or_plus_flag_arg!(DisableFilenameGlobbing, 'f', "Disable filename globbing");
builtin::minus_or_plus_flag_arg!(RememberCommandLocations, 'h', "Remember command locations");
builtin::minus_or_plus_flag_arg!(
    PlaceAllAssignmentArgsInCommandEnv,
    'k',
    "Place all assignment args in command environment"
);
builtin::minus_or_plus_flag_arg!(EnableJobControl, 'm', "Enable job control");
builtin::minus_or_plus_flag_arg!(DoNotExecuteCommands, 'n', "Do not execute commands");
builtin::minus_or_plus_flag_arg!(RealEffectiveUidMismatch, 'p', "Real effective UID mismatch");
builtin::minus_or_plus_flag_arg!(ExitAfterOneCommand, 't', "Exit after one command");
builtin::minus_or_plus_flag_arg!(
    TreatUnsetVariablesAsError,
    'u',
    "Treat unset variables as error"
);
builtin::minus_or_plus_flag_arg!(PrintShellInputLines, 'v', "Print shell input lines");
builtin::minus_or_plus_flag_arg!(
    PrintCommandsAndArguments,
    'x',
    "Print commands and arguments"
);
builtin::minus_or_plus_flag_arg!(PerformBraceExpansion, 'B', "Perform brace expansion");
builtin::minus_or_plus_flag_arg!(
    DisallowOverwritingRegularFilesViaOutputRedirection,
    'C',
    "Disallow overwriting regular files via output redirection"
);
builtin::minus_or_plus_flag_arg!(
    ShellFunctionsInheritErrTrap,
    'E',
    "Shell functions inherit ERR trap"
);
builtin::minus_or_plus_flag_arg!(
    EnableBangStyleHistorySubstitution,
    'H',
    "Enable bang style history substitution"
);
builtin::minus_or_plus_flag_arg!(
    DoNotResolveSymlinksWhenChangingDir,
    'P',
    "Do not resolve symlinks when changing dir"
);
builtin::minus_or_plus_flag_arg!(
    ShellFunctionsInheritDebugAndReturnTraps,
    'T',
    "Shell functions inherit DEBUG and RETURN traps"
);

#[derive(clap::Parser)]
pub(crate) struct SetOption {
    #[arg(short = 'o', name = "setopt_enable")]
    enable: Vec<String>,
    #[arg(long = concat!("+o"), name = "setopt_disable", hide = true)]
    disable: Vec<String>,
}

/// Manage set-based shell options.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
pub(crate) struct SetCommand {
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,

    #[clap(flatten)]
    export_variables_on_modification: ExportVariablesOnModification,
    #[clap(flatten)]
    notify_job_termination_immediately: NotfyJobTerminationImmediately,
    #[clap(flatten)]
    exit_on_nonzero_command_exit: ExitOnNonzeroCommandExit,
    #[clap(flatten)]
    disable_filename_globbing: DisableFilenameGlobbing,
    #[clap(flatten)]
    remember_command_locations: RememberCommandLocations,
    #[clap(flatten)]
    place_all_assignment_args_in_command_env: PlaceAllAssignmentArgsInCommandEnv,
    #[clap(flatten)]
    enable_job_control: EnableJobControl,
    #[clap(flatten)]
    do_not_execute_commands: DoNotExecuteCommands,
    #[clap(flatten)]
    real_effective_uid_mismatch: RealEffectiveUidMismatch,
    #[clap(flatten)]
    exit_after_one_command: ExitAfterOneCommand,
    #[clap(flatten)]
    treat_unset_variables_as_error: TreatUnsetVariablesAsError,
    #[clap(flatten)]
    print_shell_input_lines: PrintShellInputLines,
    #[clap(flatten)]
    print_commands_and_arguments: PrintCommandsAndArguments,
    #[clap(flatten)]
    perform_brace_expansion: PerformBraceExpansion,
    #[clap(flatten)]
    disallow_overwriting_regular_files_via_output_redirection:
        DisallowOverwritingRegularFilesViaOutputRedirection,
    #[clap(flatten)]
    shell_functions_inherit_err_trap: ShellFunctionsInheritErrTrap,
    #[clap(flatten)]
    enable_bang_style_history_substitution: EnableBangStyleHistorySubstitution,
    #[clap(flatten)]
    do_not_resolve_symlinks_when_changing_dir: DoNotResolveSymlinksWhenChangingDir,
    #[clap(flatten)]
    shell_functions_inherit_debug_and_return_traps: ShellFunctionsInheritDebugAndReturnTraps,

    #[clap(flatten)]
    set_option: SetOption,

    positional_args: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for SetCommand {
    fn takes_plus_options() -> bool {
        true
    }

    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtin::ExitCode, error::Error> {
        let mut result = builtin::ExitCode::Success;

        if let Some(value) = self.print_commands_and_arguments.to_bool() {
            context.shell.options.print_commands_and_arguments = value;
        }

        if let Some(value) = self.export_variables_on_modification.to_bool() {
            context.shell.options.export_variables_on_modification = value;
        }

        if let Some(value) = self.notify_job_termination_immediately.to_bool() {
            context.shell.options.notify_job_termination_immediately = value;
        }

        if let Some(value) = self.exit_on_nonzero_command_exit.to_bool() {
            context.shell.options.exit_on_nonzero_command_exit = value;
        }

        if let Some(value) = self.disable_filename_globbing.to_bool() {
            context.shell.options.disable_filename_globbing = value;
        }

        if let Some(value) = self.remember_command_locations.to_bool() {
            context.shell.options.remember_command_locations = value;
        }

        if let Some(value) = self.place_all_assignment_args_in_command_env.to_bool() {
            context
                .shell
                .options
                .place_all_assignment_args_in_command_env = value;
        }

        if let Some(value) = self.enable_job_control.to_bool() {
            context.shell.options.enable_job_control = value;
        }

        if let Some(value) = self.do_not_execute_commands.to_bool() {
            context.shell.options.do_not_execute_commands = value;
        }

        if let Some(value) = self.real_effective_uid_mismatch.to_bool() {
            context.shell.options.real_effective_uid_mismatch = value;
        }

        if let Some(value) = self.exit_after_one_command.to_bool() {
            context.shell.options.exit_after_one_command = value;
        }

        if let Some(value) = self.treat_unset_variables_as_error.to_bool() {
            context.shell.options.treat_unset_variables_as_error = value;
        }

        if let Some(value) = self.print_shell_input_lines.to_bool() {
            context.shell.options.print_shell_input_lines = value;
        }

        if let Some(value) = self.print_commands_and_arguments.to_bool() {
            context.shell.options.print_commands_and_arguments = value;
        }

        if let Some(value) = self.perform_brace_expansion.to_bool() {
            context.shell.options.perform_brace_expansion = value;
        }

        if let Some(value) = self
            .disallow_overwriting_regular_files_via_output_redirection
            .to_bool()
        {
            context
                .shell
                .options
                .disallow_overwriting_regular_files_via_output_redirection = value;
        }

        if let Some(value) = self.shell_functions_inherit_err_trap.to_bool() {
            context.shell.options.shell_functions_inherit_err_trap = value;
        }

        if let Some(value) = self.enable_bang_style_history_substitution.to_bool() {
            context.shell.options.enable_bang_style_history_substitution = value;
        }

        if let Some(value) = self.do_not_resolve_symlinks_when_changing_dir.to_bool() {
            context
                .shell
                .options
                .do_not_resolve_symlinks_when_changing_dir = value;
        }

        if let Some(value) = self
            .shell_functions_inherit_debug_and_return_traps
            .to_bool()
        {
            context
                .shell
                .options
                .shell_functions_inherit_debug_and_return_traps = value;
        }

        let mut named_options: HashMap<String, bool> = HashMap::new();
        for option_name in &self.set_option.disable {
            named_options.insert(option_name.to_owned(), false);
        }
        for option_name in &self.set_option.enable {
            named_options.insert(option_name.to_owned(), true);
        }

        for (option_name, value) in named_options {
            if let Some(option_def) = namedoptions::SET_O_OPTIONS.get(option_name.as_str()) {
                (option_def.setter)(context.shell, value);
            } else {
                result = builtin::ExitCode::InvalidUsage;
            }
        }

        for (i, arg) in self.positional_args.iter().enumerate() {
            if arg == "-" && i == 0 {
                continue;
            }

            if i < context.shell.positional_parameters.len() {
                arg.clone_into(&mut context.shell.positional_parameters[i]);
            } else {
                context.shell.positional_parameters.push(arg.to_owned());
            }
        }

        Ok(result)
    }
}
