use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExecutionContext, BuiltinExitCode};

#[derive(Parser, Debug)]
#[clap(disable_help_flag = true)]
pub(crate) struct SetCommand {
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,

    /// Export variables on modification
    #[arg(short = 'a', action = clap::ArgAction::SetTrue)]
    export_variables_on_modification: Option<bool>,
    #[arg(long = "+a", action = clap::ArgAction::SetFalse, overrides_with = "export_variables_on_modification", hide = true)]
    _export_variables_on_modification_hidden: Option<bool>,

    /// Notify job termination immediately
    #[arg(short = 'b', action = clap::ArgAction::SetTrue)]
    notify_job_termination_immediately: Option<bool>,
    #[arg(long = "+b", action = clap::ArgAction::SetFalse, overrides_with = "notify_job_termination_immediately", hide = true)]
    _notify_job_termination_immediately_hidden: Option<bool>,

    /// Exit on nonzero command exit
    #[arg(short = 'e', action = clap::ArgAction::SetTrue)]
    exit_on_nonzero_command_exit: Option<bool>,
    #[arg(long = "+e", action = clap::ArgAction::SetFalse, overrides_with = "exit_on_nonzero_command_exit", hide = true)]
    _exit_on_nonzero_command_exit_hidden: Option<bool>,

    /// Disable filename globbing
    #[arg(short = 'f', action = clap::ArgAction::SetTrue)]
    disable_filename_globbing: Option<bool>,
    #[arg(long = "+f", action = clap::ArgAction::SetFalse, overrides_with = "disable_filename_globbing", hide = true)]
    _disable_filename_globbing_hidden: Option<bool>,

    /// Remember command locations
    #[arg(short = 'h', action = clap::ArgAction::SetTrue)]
    remember_command_locations: Option<bool>,
    #[arg(long = "+h", action = clap::ArgAction::SetFalse, overrides_with = "remember_command_locations", hide = true)]
    _remember_command_locations_hidden: Option<bool>,

    /// Place all assignment args in command environment
    #[arg(short = 'k', action = clap::ArgAction::SetTrue)]
    place_all_assignment_args_in_command_env: Option<bool>,
    #[arg(long = "+k", action = clap::ArgAction::SetFalse, overrides_with = "place_all_assignment_args_in_command_env", hide = true)]
    _place_all_assignment_args_in_command_env_hidden: Option<bool>,

    /// Enable job control
    #[arg(short = 'm', action = clap::ArgAction::SetTrue)]
    enable_job_control: Option<bool>,
    #[arg(long = "+m", action = clap::ArgAction::SetFalse, overrides_with = "enable_job_control", hide = true)]
    _enable_job_control_hidden: Option<bool>,

    /// Do not execute commands
    #[arg(short = 'n', action = clap::ArgAction::SetTrue)]
    do_not_execute_commands: Option<bool>,
    #[arg(long = "+n", action = clap::ArgAction::SetFalse, overrides_with = "do_not_execute_commands", hide = true)]
    _do_not_execute_commands_hidden: Option<bool>,

    /// Real effective UID mismatch
    #[arg(short = 'p', action = clap::ArgAction::SetTrue)]
    real_effective_uid_mismatch: Option<bool>,
    #[arg(long = "+p", action = clap::ArgAction::SetFalse, overrides_with = "real_effective_uid_mismatch", hide = true)]
    _real_effective_uid_mismatch_hidden: Option<bool>,

    /// Exit after one command
    #[arg(short = 't', action = clap::ArgAction::SetTrue)]
    exit_after_one_command: Option<bool>,
    #[arg(long = "+t", action = clap::ArgAction::SetFalse, overrides_with = "exit_after_one_command", hide = true)]
    _exit_after_one_command_hidden: Option<bool>,

    /// Treat unset variables as error
    #[arg(short = 'u', action = clap::ArgAction::SetTrue)]
    treat_unset_variables_as_error: Option<bool>,
    #[arg(long = "+u", action = clap::ArgAction::SetFalse, overrides_with = "treat_unset_variables_as_error", hide = true)]
    _treat_unset_variables_as_error_hidden: Option<bool>,

    /// Print shell input lines
    #[arg(short = 'v', action = clap::ArgAction::SetTrue)]
    print_shell_input_lines: Option<bool>,
    #[arg(long = "+v", action = clap::ArgAction::SetFalse, overrides_with = "print_shell_input_lines", hide = true)]
    _print_shell_input_lines_hidden: Option<bool>,

    /// Print commands and arguments
    #[arg(short = 'x', action = clap::ArgAction::SetTrue)]
    print_commands_and_arguments: Option<bool>,
    #[arg(long = "+x", action = clap::ArgAction::SetFalse, overrides_with = "print_commands_and_arguments", hide = true)]
    _print_commands_and_arguments_hidden: Option<bool>,

    /// Perform brace expansion
    #[arg(short = 'B', action = clap::ArgAction::SetTrue)]
    perform_brace_expansion: Option<bool>,
    #[arg(long = "+B", action = clap::ArgAction::SetFalse, overrides_with = "perform_brace_expansion", hide = true)]
    _perform_brace_expansion_hidden: Option<bool>,

    /// Disallow overwriting regular files via output redirection
    #[arg(short = 'C', action = clap::ArgAction::SetTrue)]
    disallow_overwriting_regular_files_via_output_redirection: Option<bool>,
    #[arg(long = "+C", action = clap::ArgAction::SetFalse, overrides_with = "disallow_overwriting_regular_files_via_output_redirection", hide = true)]
    _disallow_overwriting_regular_files_via_output_redirection_hidden: Option<bool>,

    /// Shell functions inherit ERR trap
    #[arg(short = 'E', action = clap::ArgAction::SetTrue)]
    shell_functions_inherit_err_trap: Option<bool>,
    #[arg(long = "+E", action = clap::ArgAction::SetFalse, overrides_with = "shell_functions_inherit_err_trap", hide = true)]
    _shell_functions_inherit_err_trap_hidden: Option<bool>,

    /// Enable bang style history substitution
    #[arg(short = 'H', action = clap::ArgAction::SetTrue)]
    enable_bang_style_history_substitution: Option<bool>,
    #[arg(long = "+H", action = clap::ArgAction::SetFalse, overrides_with = "enable_bang_style_history_substitution", hide = true)]
    _enable_bang_style_history_substitution_hidden: Option<bool>,

    /// Do not resolve symlinks when changing dir
    #[arg(short = 'P', action = clap::ArgAction::SetTrue)]
    do_not_resolve_symlinks_when_changing_dir: Option<bool>,
    #[arg(long = "+P", action = clap::ArgAction::SetFalse, overrides_with = "do_not_resolve_symlinks_when_changing_dir", hide = true)]
    _do_not_resolve_symlinks_when_changing_dir_hidden: Option<bool>,

    /// Shell functions inherit DEBUG and RETURN traps
    #[arg(short = 'T', action = clap::ArgAction::SetTrue)]
    shell_functions_inherit_debug_and_return_traps: Option<bool>,
    #[arg(long = "+T", action = clap::ArgAction::SetFalse, overrides_with = "shell_functions_inherit_debug_and_return_traps", hide = true)]
    _shell_functions_inherit_debug_and_return_traps_hidden: Option<bool>,

    // TODO: implement: -o
    // TODO: implement: --
    // TODO: implement: -
    #[clap(allow_hyphen_values = true)]
    unhandled_args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for SetCommand {
    async fn execute(&self, context: &mut BuiltinExecutionContext<'_>) -> Result<BuiltinExitCode> {
        if let Some(value) = self.print_commands_and_arguments {
            context.shell.options.print_commands_and_arguments = value;
        }

        if !self.unhandled_args.is_empty() {
            log::error!("UNIMPLEMENTED: set builtin received unhandled arguments");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        Ok(BuiltinExitCode::Success)
    }
}
