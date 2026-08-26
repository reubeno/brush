//! `set` builtin: `SetCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::ffi::OsStr;

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

crate::tri_state_flag!(ExportVariablesOnModification);
crate::tri_state_flag!(NotifyJobTerminationImmediately);
crate::tri_state_flag!(ExitOnNonzeroCommandExit);
crate::tri_state_flag!(DisableFilenameGlobbing);
crate::tri_state_flag!(RememberCommandLocations);
crate::tri_state_flag!(PlaceAllAssignmentArgsInCommandEnv);
crate::tri_state_flag!(EnableJobControl);
crate::tri_state_flag!(DoNotExecuteCommands);
crate::tri_state_flag!(RealEffectiveUidMismatch);
crate::tri_state_flag!(ExitAfterOneCommand);
crate::tri_state_flag!(TreatUnsetVariablesAsError);
crate::tri_state_flag!(PrintShellInputLines);
crate::tri_state_flag!(PrintCommandsAndArguments);
crate::tri_state_flag!(PerformBraceExpansion);
crate::tri_state_flag!(DisallowOverwritingRegularFilesViaOutputRedirection);
crate::tri_state_flag!(ShellFunctionsInheritErrTrap);
crate::tri_state_flag!(EnableBangStyleHistorySubstitution);
crate::tri_state_flag!(DoNotResolveSymlinksWhenChangingDir);
crate::tri_state_flag!(ShellFunctionsInheritDebugAndReturnTraps);

/// Returns a parser capturing repeated occurrences of a named-option flag
/// (e.g., `-o OPT`) into the same tri-state shape used by [`SetOption`].
fn named_option_section<P>(flag: P) -> impl bpaf::Parser<Option<Vec<String>>>
where
    P: bpaf::Parser<()> + 'static,
{
    let value = bpaf::any("OPT", |s: String| {
        if s.starts_with('-') || s.starts_with('+') {
            None
        } else {
            Some(s)
        }
    })
    .optional();

    use bpaf::Parser;

    // N.B. The construct consumes the flag, so absence of the flag yields
    // `None` via `optional()`; an empty vector then means "flag given without
    // a value" (bash's list-all form). Gating on the flag here is essential:
    // an alternative that could succeed without it would make every
    // invocation look like a list request.
    bpaf::construct!(flag, value)
        .map(|((), v)| v.map(|v| vec![v]).unwrap_or_default())
        .optional()
}

/// Tri-state parser for a `-x` / `+x` option pair.
fn minus_or_plus_flag(
    flag_char: char,
    plus_form: &'static str,
    desc: &'static str,
) -> impl bpaf::Parser<Option<bool>> {
    use bpaf::Parser;

    let enable = bpaf::short(flag_char)
        .help(desc)
        .switch()
        .map(|enabled| enabled.then_some(true));
    let disable = bpaf::literal(plus_form)
        .help("Disables the flag.")
        .hide()
        .map(|(): ()| Some(false));

    bpaf::construct!([enable, disable]).fallback(None)
}

/// Set or unset shell positional arguments and options.
pub(crate) struct SetCommand {
    pub(super) export_variables_on_modification: ExportVariablesOnModification,
    pub(super) notify_job_termination_immediately: NotifyJobTerminationImmediately,
    pub(super) exit_on_nonzero_command_exit: ExitOnNonzeroCommandExit,
    pub(super) disable_filename_globbing: DisableFilenameGlobbing,
    pub(super) remember_command_locations: RememberCommandLocations,
    pub(super) place_all_assignment_args_in_command_env: PlaceAllAssignmentArgsInCommandEnv,
    pub(super) enable_job_control: EnableJobControl,
    pub(super) do_not_execute_commands: DoNotExecuteCommands,
    pub(super) real_effective_uid_mismatch: RealEffectiveUidMismatch,
    pub(super) exit_after_one_command: ExitAfterOneCommand,
    pub(super) treat_unset_variables_as_error: TreatUnsetVariablesAsError,
    pub(super) print_shell_input_lines: PrintShellInputLines,
    pub(super) print_commands_and_arguments: PrintCommandsAndArguments,
    pub(super) perform_brace_expansion: PerformBraceExpansion,
    pub(super) disallow_overwriting_regular_files_via_output_redirection:
        DisallowOverwritingRegularFilesViaOutputRedirection,
    pub(super) shell_functions_inherit_err_trap: ShellFunctionsInheritErrTrap,
    pub(super) enable_bang_style_history_substitution: EnableBangStyleHistorySubstitution,
    pub(super) do_not_resolve_symlinks_when_changing_dir: DoNotResolveSymlinksWhenChangingDir,
    pub(super) shell_functions_inherit_debug_and_return_traps:
        ShellFunctionsInheritDebugAndReturnTraps,
    pub(super) set_option: SetOption,
    pub(super) positional_args: Vec<String>,
    pub(super) help: Option<bool>,
}

/// The `-o`/`+o` named-option argument.
pub(crate) struct SetOption {
    /// Named options enabled via `-o`.
    pub(super) enable: Option<Vec<String>>,
    /// Named options disabled via `+o`.
    pub(super) disable: Option<Vec<String>>,
}

impl crate::args::bpaf_support::BpafArgs for SetCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        let export_variables_on_modification =
            minus_or_plus_flag('a', "+a", "Export variables on modification")
                .map(ExportVariablesOnModification::from_bool);
        let notify_job_termination_immediately =
            minus_or_plus_flag('b', "+b", "Notify job termination immediately")
                .map(NotifyJobTerminationImmediately::from_bool);
        let exit_on_nonzero_command_exit =
            minus_or_plus_flag('e', "+e", "Exit on nonzero command exit")
                .map(ExitOnNonzeroCommandExit::from_bool);
        let disable_filename_globbing = minus_or_plus_flag('f', "+f", "Disable filename globbing")
            .map(DisableFilenameGlobbing::from_bool);
        let remember_command_locations = minus_or_plus_flag('h', "+h", "Remember command locations")
            .map(RememberCommandLocations::from_bool);
        let place_all_assignment_args_in_command_env = minus_or_plus_flag(
            'k',
            "+k",
            "Place all assignment args in command environment",
        )
        .map(PlaceAllAssignmentArgsInCommandEnv::from_bool);
        let enable_job_control =
            minus_or_plus_flag('m', "+m", "Enable job control").map(EnableJobControl::from_bool);
        let do_not_execute_commands =
            minus_or_plus_flag('n', "+n", "Do not execute commands")
                .map(DoNotExecuteCommands::from_bool);
        let real_effective_uid_mismatch =
            minus_or_plus_flag('p', "+p", "Real effective UID mismatch")
                .map(RealEffectiveUidMismatch::from_bool);
        let exit_after_one_command = minus_or_plus_flag('t', "+t", "Exit after one command")
            .map(ExitAfterOneCommand::from_bool);
        let treat_unset_variables_as_error =
            minus_or_plus_flag('u', "+u", "Treat unset variables as error")
                .map(TreatUnsetVariablesAsError::from_bool);
        let print_shell_input_lines = minus_or_plus_flag('v', "+v", "Print shell input lines")
            .map(PrintShellInputLines::from_bool);
        let print_commands_and_arguments = minus_or_plus_flag(
            'x',
            "+x",
            "Print commands and arguments",
        )
        .map(PrintCommandsAndArguments::from_bool);
        let perform_brace_expansion = minus_or_plus_flag('B', "+B", "Perform brace expansion")
            .map(PerformBraceExpansion::from_bool);
        let disallow_overwriting_regular_files_via_output_redirection = minus_or_plus_flag(
            'C',
            "+C",
            "Disallow overwriting regular files via output redirection",
        )
        .map(DisallowOverwritingRegularFilesViaOutputRedirection::from_bool);
        let shell_functions_inherit_err_trap =
            minus_or_plus_flag('E', "+E", "Shell functions inherit ERR trap")
                .map(ShellFunctionsInheritErrTrap::from_bool);
        let enable_bang_style_history_substitution =
            minus_or_plus_flag('H', "+H", "Enable bang style history substitution")
                .map(EnableBangStyleHistorySubstitution::from_bool);
        let do_not_resolve_symlinks_when_changing_dir =
            minus_or_plus_flag('P', "+P", "Do not resolve symlinks when changing dir")
                .map(DoNotResolveSymlinksWhenChangingDir::from_bool);
        let shell_functions_inherit_debug_and_return_traps =
            minus_or_plus_flag('T', "+T", "Shell functions inherit DEBUG and RETURN traps")
                .map(ShellFunctionsInheritDebugAndReturnTraps::from_bool);

        let set_option = {
            let enable = named_option_section(
                bpaf::short('o')
                    .help("Specify a named option; without OPT, lists all named options.")
                    .req_flag(()),
            );
            let disable = named_option_section(bpaf::literal("+o"));

            bpaf::construct!(SetOption { enable, disable })
        };

        // N.B. Trailing arguments are captured verbatim via `takes_trailing_args`.
        let positional_args = bpaf::pure(Vec::new());
        let help = bpaf::pure(None);

        bpaf::construct!(SetCommand {
            export_variables_on_modification,
            notify_job_termination_immediately,
            exit_on_nonzero_command_exit,
            disable_filename_globbing,
            remember_command_locations,
            place_all_assignment_args_in_command_env,
            enable_job_control,
            do_not_execute_commands,
            real_effective_uid_mismatch,
            exit_after_one_command,
            treat_unset_variables_as_error,
            print_shell_input_lines,
            print_commands_and_arguments,
            perform_brace_expansion,
            disallow_overwriting_regular_files_via_output_redirection,
            shell_functions_inherit_err_trap,
            enable_bang_style_history_substitution,
            do_not_resolve_symlinks_when_changing_dir,
            shell_functions_inherit_debug_and_return_traps,
            set_option,
            positional_args,
            help,
        })
    }

    fn takes_plus_options() -> bool {
        true
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn value_taking_short_options() -> &'static str {
        "o"
    }

    /// Overrides the default flow so the presence of a bare `--` terminator is
    /// folded into the trailing operands (mirroring bash's `set --`), and
    /// `-oVALUE` groups are expanded before splitting.
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let mut args: Vec<String> = words.to_vec();

        // N.B. The first word is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let mut expanded = Vec::with_capacity(args.len());
        for arg in args {
            match arg.strip_prefix('+').filter(|group| !group.is_empty()) {
                Some(group) if !group.starts_with('+') && !group.contains('=') => {
                    expanded.extend(group.chars().map(|c| format!("+{c}")));
                }
                _ => expanded.extend(expand_dash_group(&arg)),
            }
        }

        let (options, trailing) =
            crate::args::bpaf_support::split_option_section(&expanded, Self::value_taking_short_options(), &[]);

        let os_args: Vec<&OsStr> = options.iter().map(OsStr::new).collect();
        let mut command = Self::parser()
            .to_options()
            .run_inner(os_args.as_slice())
            .map_err(crate::args::bpaf_support::render_parse_failure)?;

        command.set_trailing_args(trailing);

        Ok(command)
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        self.positional_args = args;
    }


}
impl FromArgs for SetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for SetCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}

/// Expands a single leading-dash token into bpaf-parseable tokens.
///
/// bpaf has no POSIX short-option clustering support, so multi-letter groups
/// like `-euo` are split into individual switches (`-e`, `-u`, `-o`); a group
/// ending in (or containing) the value-taking option `o` yields `-o` plus its
/// attached value. Anything else (single switches, `--long`, bare `-`, or
/// unrecognized shapes) passes through unchanged.
fn expand_dash_group(arg: &str) -> Vec<String> {
    let rest = match arg.strip_prefix('-') {
        // Long options, bare "-", and empty groups are not clusters.
        Some(r) if !r.is_empty() && !r.starts_with('-') => r,
        _ => return vec![arg.to_string()],
    };

    // Clusters are plain letter runs; anything else (attached values via '=',
    // digits, etc.) is left for the narrower `-oVALUE` handler below.
    if rest.contains('=') || !rest.chars().all(|c| c.is_ascii_alphabetic()) {
        return expand_dash_o_group(arg);
    }

    let mut expanded = Vec::new();
    let mut pending = String::new();
    for c in rest.chars() {
        if c == 'o' {
            if !pending.is_empty() {
                expanded.push(format!("-{pending}"));
                pending.clear();
            }
            expanded.push(String::from("-o"));
            // N.B. any letters after `o` belong to its value, which the
            // splitter/parser handle as the following token.
            break;
        }
        pending.push(c);
    }
    if !pending.is_empty() {
        expanded.push(format!("-{pending}"));
    }

    expanded
}

/// Expands a `-oVALUE` token into separate tokens (`-o`, `VALUE`) so the
/// splitter can treat them correctly.
fn expand_dash_o_group(arg: &str) -> Vec<String> {
    if let Some(rest) = arg.strip_prefix("-o") {
        if !rest.is_empty() && !rest.starts_with('-') && !rest.starts_with('+') {
            return vec![String::from("-o"), rest.to_string()];
        }
    }

    vec![arg.to_string()]
}
