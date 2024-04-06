use anyhow::Result;
use clap::Parser;

use crate::builtin::{self, BuiltinCommand, BuiltinExitCode};
use crate::error;

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

    // TODO: implement: -o
    // TODO: implement: --
    // TODO: implement: -
    #[clap(allow_hyphen_values = true)]
    unhandled_args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for SetCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<BuiltinExitCode, error::Error> {
        if let Some(value) = self.print_commands_and_arguments.to_bool() {
            context.shell.options.print_commands_and_arguments = value;
        }

        if !self.unhandled_args.is_empty() {
            log::error!("UNIMPLEMENTED: set builtin received unhandled arguments");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        Ok(BuiltinExitCode::Success)
    }
}
