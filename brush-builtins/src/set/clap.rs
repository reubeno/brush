//! `set` builtin: `SetCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage set-based shell options.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
pub(crate) struct SetCommand {
    /// Display help for this command.
    #[clap(long, action = clap::ArgAction::HelpLong)]
    pub(super) help: Option<bool>,

    #[clap(flatten)]
    pub(super) export_variables_on_modification: ExportVariablesOnModification,
    #[clap(flatten)]
    pub(super) notify_job_termination_immediately: NotifyJobTerminationImmediately,
    #[clap(flatten)]
    pub(super) exit_on_nonzero_command_exit: ExitOnNonzeroCommandExit,
    #[clap(flatten)]
    pub(super) disable_filename_globbing: DisableFilenameGlobbing,
    #[clap(flatten)]
    pub(super) remember_command_locations: RememberCommandLocations,
    #[clap(flatten)]
    pub(super) place_all_assignment_args_in_command_env: PlaceAllAssignmentArgsInCommandEnv,
    #[clap(flatten)]
    pub(super) enable_job_control: EnableJobControl,
    #[clap(flatten)]
    pub(super) do_not_execute_commands: DoNotExecuteCommands,
    #[clap(flatten)]
    pub(super) real_effective_uid_mismatch: RealEffectiveUidMismatch,
    #[clap(flatten)]
    pub(super) exit_after_one_command: ExitAfterOneCommand,
    #[clap(flatten)]
    pub(super) treat_unset_variables_as_error: TreatUnsetVariablesAsError,
    #[clap(flatten)]
    pub(super) print_shell_input_lines: PrintShellInputLines,
    #[clap(flatten)]
    pub(super) print_commands_and_arguments: PrintCommandsAndArguments,
    #[clap(flatten)]
    pub(super) perform_brace_expansion: PerformBraceExpansion,
    #[clap(flatten)]
    pub(super) disallow_overwriting_regular_files_via_output_redirection:
        DisallowOverwritingRegularFilesViaOutputRedirection,
    #[clap(flatten)]
    pub(super) shell_functions_inherit_err_trap: ShellFunctionsInheritErrTrap,
    #[clap(flatten)]
    pub(super) enable_bang_style_history_substitution: EnableBangStyleHistorySubstitution,
    #[clap(flatten)]
    pub(super) do_not_resolve_symlinks_when_changing_dir: DoNotResolveSymlinksWhenChangingDir,
    #[clap(flatten)]
    pub(super) shell_functions_inherit_debug_and_return_traps: ShellFunctionsInheritDebugAndReturnTraps,

    #[clap(flatten)]
    pub(super) set_option: SetOption,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(super) positional_args: Vec<String>,
}

#[derive(clap::Parser)]

pub(super) struct SetOption {
    #[arg(short = 'o', name = "setopt_enable", num_args=0..=1, value_name = "OPT")]
    pub(super) enable: Option<Vec<String>>,
    #[arg(long = concat!("+o"), name = "setopt_disable", hide = true, num_args=0..=1)]
    pub(super) disable: Option<Vec<String>>,
}


crate::minus_or_plus_flag_arg!(
    ExportVariablesOnModification,
    'a',
    "Export variables on modification"
);

crate::minus_or_plus_flag_arg!(
    NotifyJobTerminationImmediately,
    'b',
    "Notify job termination immediately"
);

crate::minus_or_plus_flag_arg!(
    ExitOnNonzeroCommandExit,
    'e',
    "Exit on nonzero command exit"
);

crate::minus_or_plus_flag_arg!(DisableFilenameGlobbing, 'f', "Disable filename globbing");

crate::minus_or_plus_flag_arg!(RememberCommandLocations, 'h', "Remember command locations");

crate::minus_or_plus_flag_arg!(
    PlaceAllAssignmentArgsInCommandEnv,
    'k',
    "Place all assignment args in command environment"
);

crate::minus_or_plus_flag_arg!(EnableJobControl, 'm', "Enable job control");

crate::minus_or_plus_flag_arg!(DoNotExecuteCommands, 'n', "Do not execute commands");

crate::minus_or_plus_flag_arg!(RealEffectiveUidMismatch, 'p', "Real effective UID mismatch");

crate::minus_or_plus_flag_arg!(ExitAfterOneCommand, 't', "Exit after one command");

crate::minus_or_plus_flag_arg!(
    TreatUnsetVariablesAsError,
    'u',
    "Treat unset variables as error"
);

crate::minus_or_plus_flag_arg!(PrintShellInputLines, 'v', "Print shell input lines");

crate::minus_or_plus_flag_arg!(
    PrintCommandsAndArguments,
    'x',
    "Print commands and arguments"
);

crate::minus_or_plus_flag_arg!(PerformBraceExpansion, 'B', "Perform brace expansion");

crate::minus_or_plus_flag_arg!(
    DisallowOverwritingRegularFilesViaOutputRedirection,
    'C',
    "Disallow overwriting regular files via output redirection"
);

crate::minus_or_plus_flag_arg!(
    ShellFunctionsInheritErrTrap,
    'E',
    "Shell functions inherit ERR trap"
);

crate::minus_or_plus_flag_arg!(
    EnableBangStyleHistorySubstitution,
    'H',
    "Enable bang style history substitution"
);

crate::minus_or_plus_flag_arg!(
    DoNotResolveSymlinksWhenChangingDir,
    'P',
    "Do not resolve symlinks when changing dir"
);

crate::minus_or_plus_flag_arg!(
    ShellFunctionsInheritDebugAndReturnTraps,
    'T',
    "Shell functions inherit DEBUG and RETURN traps"
);

impl builtins::Command for SetCommand {
    fn takes_plus_options() -> bool {
        true
    }

    /// Override the default [`builtins::Command::new`] function to handle clap's limitation related
    /// to `--`. See [`builtins::parse_known`] for more information
    /// TODO(set): we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, brush_core::args::ArgsError>
    where
        I: IntoIterator<Item = String>,
    {
        //
        // TODO(set): This is getting pretty messy; we need to see how to avoid this -- handling
        // from leaking into too many commands' custom parsing.
        //

        // Apply the same workaround from the default implementation of Command::new to handle '+'
        // args.
        let mut updated_args = vec![];
        let mut now_parsing_positional_args = false;
        let mut next_arg_is_option_value = false;
        for (i, arg) in args.into_iter().enumerate() {
            if now_parsing_positional_args || next_arg_is_option_value {
                updated_args.push(arg);

                next_arg_is_option_value = false;
                continue;
            }

            if arg == "-" || arg == "--" || (i > 0 && !arg.starts_with(['-', '+'])) {
                now_parsing_positional_args = true;
            }

            if let Some(plus_options) = arg.strip_prefix("+") {
                next_arg_is_option_value = plus_options.ends_with('o');
                for c in plus_options.chars() {
                    updated_args.push(format!("--+{c}"));
                }
            } else {
                next_arg_is_option_value = arg.starts_with('-') && arg.ends_with('o');
                updated_args.push(arg);
            }
        }

        let (mut this, rest_args) = brush_core::builtins::try_parse_known::<Self>(updated_args)
            .map_err(|err| brush_core::args::ArgsError::from_clap_error(&err))?;
        if let Some(args) = rest_args {
            this.positional_args.extend(args);
        }
        Ok(this)
    }

    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
