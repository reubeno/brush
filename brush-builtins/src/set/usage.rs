//! `set` builtin: `SetCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::collections::HashMap;
use std::io::Write;
use itertools::Itertools;
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};
use brush_core::args::{ArgsError, FromArgs};

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
