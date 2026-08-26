//! `set` builtin: `SetCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use itertools::Itertools;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

crate::usage_minus_or_plus_flag_arg!(
    ExportVariablesOnModification,
    'a',
    "+a",
    "Export variables on modification"
);
crate::usage_minus_or_plus_flag_arg!(
    NotifyJobTerminationImmediately,
    'b',
    "+b",
    "Notify job termination immediately"
);
crate::usage_minus_or_plus_flag_arg!(
    ExitOnNonzeroCommandExit,
    'e',
    "+e",
    "Exit on nonzero command exit"
);
crate::usage_minus_or_plus_flag_arg!(
    DisableFilenameGlobbing,
    'f',
    "+f",
    "Disable filename globbing"
);
crate::usage_minus_or_plus_flag_arg!(
    RememberCommandLocations,
    'h',
    "+h",
    "Remember command locations"
);
crate::usage_minus_or_plus_flag_arg!(
    PlaceAllAssignmentArgsInCommandEnv,
    'k',
    "+k",
    "Place all assignment args in command environment"
);
crate::usage_minus_or_plus_flag_arg!(EnableJobControl, 'm', "+m", "Enable job control");
crate::usage_minus_or_plus_flag_arg!(DoNotExecuteCommands, 'n', "+n", "Do not execute commands");
crate::usage_minus_or_plus_flag_arg!(
    RealEffectiveUidMismatch,
    'p',
    "+p",
    "Real effective UID mismatch"
);
crate::usage_minus_or_plus_flag_arg!(ExitAfterOneCommand, 't', "+t", "Exit after one command");
crate::usage_minus_or_plus_flag_arg!(
    TreatUnsetVariablesAsError,
    'u',
    "+u",
    "Treat unset variables as error"
);
crate::usage_minus_or_plus_flag_arg!(PrintShellInputLines, 'v', "+v", "Print shell input lines");
crate::usage_minus_or_plus_flag_arg!(
    PrintCommandsAndArguments,
    'x',
    "+x",
    "Print commands and arguments"
);
crate::usage_minus_or_plus_flag_arg!(PerformBraceExpansion, 'B', "+B", "Perform brace expansion");
crate::usage_minus_or_plus_flag_arg!(
    DisallowOverwritingRegularFilesViaOutputRedirection,
    'C',
    "+C",
    "Disallow overwriting regular files via output redirection"
);
crate::usage_minus_or_plus_flag_arg!(
    ShellFunctionsInheritErrTrap,
    'E',
    "+E",
    "Shell functions inherit ERR trap"
);
crate::usage_minus_or_plus_flag_arg!(
    EnableBangStyleHistorySubstitution,
    'H',
    "+H",
    "Enable bang style history substitution"
);
crate::usage_minus_or_plus_flag_arg!(
    DoNotResolveSymlinksWhenChangingDir,
    'P',
    "+P",
    "Do not resolve symlinks when changing dir"
);
crate::usage_minus_or_plus_flag_arg!(
    ShellFunctionsInheritDebugAndReturnTraps,
    'T',
    "+T",
    "Shell functions inherit DEBUG and RETURN traps"
);

/// Sentinel bound to a bare `-o`/`+o` occurrence (a list-all request). Chosen to be
/// unspellable from shell input so it can never collide with a real option name.
const BARE_OPTION: &str = "\u{0}";

#[derive(usage::Args)]
pub(crate) struct SetOption {
    // N.B. bare `-o`/`+o` occurrences (list-all requests) are rewritten to carry
    // [`BARE_OPTION`] as an attached value by `SetCommand::new`; named occurrences
    // accumulate like they did under the previous clap-based parser.
    #[usage(short = 'o', value_name = "OPT")]
    pub(super) enable: Option<Vec<String>>,
    #[usage(long = "+o")]
    #[usage(hide, value_name = "OPT")]
    pub(super) disable: Option<Vec<String>>,
}

/// Manage set-based shell options.
#[derive(usage::Cli)]
#[usage(
    bin = "set",
    unknown_flags = "error",
    args_override_self = false,
    disable_help_flag
)]
pub(crate) struct SetCommand {
    /// Display help for this command.
    #[usage(long, action = usage::ArgAction::HelpLong)]
    pub(super) help: bool,

    #[usage(flatten)]
    pub(super) export_variables_on_modification: ExportVariablesOnModification,
    #[usage(flatten)]
    pub(super) notify_job_termination_immediately: NotifyJobTerminationImmediately,
    #[usage(flatten)]
    pub(super) exit_on_nonzero_command_exit: ExitOnNonzeroCommandExit,
    #[usage(flatten)]
    pub(super) disable_filename_globbing: DisableFilenameGlobbing,
    #[usage(flatten)]
    pub(super) remember_command_locations: RememberCommandLocations,
    #[usage(flatten)]
    pub(super) place_all_assignment_args_in_command_env: PlaceAllAssignmentArgsInCommandEnv,
    #[usage(flatten)]
    pub(super) enable_job_control: EnableJobControl,
    #[usage(flatten)]
    pub(super) do_not_execute_commands: DoNotExecuteCommands,
    #[usage(flatten)]
    pub(super) real_effective_uid_mismatch: RealEffectiveUidMismatch,
    #[usage(flatten)]
    pub(super) exit_after_one_command: ExitAfterOneCommand,
    #[usage(flatten)]
    pub(super) treat_unset_variables_as_error: TreatUnsetVariablesAsError,
    #[usage(flatten)]
    pub(super) print_shell_input_lines: PrintShellInputLines,
    #[usage(flatten)]
    pub(super) print_commands_and_arguments: PrintCommandsAndArguments,
    #[usage(flatten)]
    pub(super) perform_brace_expansion: PerformBraceExpansion,
    #[usage(flatten)]
    pub(super) disallow_overwriting_regular_files_via_output_redirection:
        DisallowOverwritingRegularFilesViaOutputRedirection,
    #[usage(flatten)]
    pub(super) shell_functions_inherit_err_trap: ShellFunctionsInheritErrTrap,
    #[usage(flatten)]
    pub(super) enable_bang_style_history_substitution: EnableBangStyleHistorySubstitution,
    #[usage(flatten)]
    pub(super) do_not_resolve_symlinks_when_changing_dir: DoNotResolveSymlinksWhenChangingDir,
    #[usage(flatten)]
    pub(super) shell_functions_inherit_debug_and_return_traps: ShellFunctionsInheritDebugAndReturnTraps,

    #[usage(flatten)]
    pub(super) set_option: SetOption,

    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) positional_args: Vec<String>,
}

crate::impl_usage_parse!(SetCommand);

impl FromArgs for SetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for SetCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
