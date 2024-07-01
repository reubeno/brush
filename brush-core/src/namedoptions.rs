use lazy_static::lazy_static;
use std::collections::HashMap;

use crate::options::RuntimeOptions;

pub(crate) type OptionGetter = fn(shell: &RuntimeOptions) -> bool;
pub(crate) type OptionSetter = fn(shell: &mut RuntimeOptions, value: bool) -> ();

pub(crate) struct OptionDefinition {
    pub getter: OptionGetter,
    pub setter: OptionSetter,
}

impl OptionDefinition {
    fn new(getter: OptionGetter, setter: OptionSetter) -> OptionDefinition {
        OptionDefinition { getter, setter }
    }
}

lazy_static! {
    pub(crate) static ref SET_OPTIONS: HashMap<char, OptionDefinition> = HashMap::from([
        (
            'a',
            OptionDefinition::new(
                |options| options.export_variables_on_modification,
                |options, value| options.export_variables_on_modification = value
            )
        ),
        (
            'b',
            OptionDefinition::new(
                |options| options.notify_job_termination_immediately,
                |options, value| options.notify_job_termination_immediately = value
            )
        ),
        (
            'e',
            OptionDefinition::new(
                |options| options.exit_on_nonzero_command_exit,
                |options, value| options.exit_on_nonzero_command_exit = value
            )
        ),
        (
            'f',
            OptionDefinition::new(
                |options| options.disable_filename_globbing,
                |options, value| options.disable_filename_globbing = value
            )
        ),
        (
            'h',
            OptionDefinition::new(
                |options| options.remember_command_locations,
                |options, value| options.remember_command_locations = value
            )
        ),
        (
            'i',
            OptionDefinition::new(
                |options| options.interactive,
                |options, value| options.interactive = value
            )
        ),
        (
            'k',
            OptionDefinition::new(
                |options| options.place_all_assignment_args_in_command_env,
                |options, value| options.place_all_assignment_args_in_command_env = value
            )
        ),
        (
            'm',
            OptionDefinition::new(
                |options| options.enable_job_control,
                |options, value| options.enable_job_control = value
            )
        ),
        (
            'n',
            OptionDefinition::new(
                |options| options.do_not_execute_commands,
                |options, value| options.do_not_execute_commands = value
            )
        ),
        (
            'p',
            OptionDefinition::new(
                |options| options.real_effective_uid_mismatch,
                |options, value| options.real_effective_uid_mismatch = value
            )
        ),
        (
            't',
            OptionDefinition::new(
                |options| options.exit_after_one_command,
                |options, value| options.exit_after_one_command = value
            )
        ),
        (
            'u',
            OptionDefinition::new(
                |options| options.treat_unset_variables_as_error,
                |options, value| options.treat_unset_variables_as_error = value
            )
        ),
        (
            'v',
            OptionDefinition::new(
                |options| options.print_shell_input_lines,
                |options, value| options.print_shell_input_lines = value
            )
        ),
        (
            'x',
            OptionDefinition::new(
                |options| options.print_commands_and_arguments,
                |options, value| options.print_commands_and_arguments = value
            )
        ),
        (
            'B',
            OptionDefinition::new(
                |options| options.perform_brace_expansion,
                |options, value| options.perform_brace_expansion = value
            )
        ),
        (
            'C',
            OptionDefinition::new(
                |options| options.disallow_overwriting_regular_files_via_output_redirection,
                |options, value| options
                    .disallow_overwriting_regular_files_via_output_redirection =
                    value
            )
        ),
        (
            'E',
            OptionDefinition::new(
                |options| options.shell_functions_inherit_err_trap,
                |options, value| options.shell_functions_inherit_err_trap = value
            )
        ),
        (
            'H',
            OptionDefinition::new(
                |options| options.enable_bang_style_history_substitution,
                |options, value| options.enable_bang_style_history_substitution = value
            )
        ),
        (
            'P',
            OptionDefinition::new(
                |options| options.do_not_resolve_symlinks_when_changing_dir,
                |options, value| options.do_not_resolve_symlinks_when_changing_dir = value
            )
        ),
        (
            'T',
            OptionDefinition::new(
                |options| options.shell_functions_inherit_debug_and_return_traps,
                |options, value| options.shell_functions_inherit_debug_and_return_traps = value
            )
        ),
        (
            's',
            OptionDefinition::new(
                |options| options.read_commands_from_stdin,
                |options, value| options.read_commands_from_stdin = value
            )
        ),
    ]);
    pub(crate) static ref SET_O_OPTIONS: HashMap<&'static str, OptionDefinition> = HashMap::from([
        (
            "allexport",
            OptionDefinition::new(
                |options| options.export_variables_on_modification,
                |options, value| options.export_variables_on_modification = value
            )
        ),
        (
            "braceexpand",
            OptionDefinition::new(
                |options| options.perform_brace_expansion,
                |options, value| options.perform_brace_expansion = value
            )
        ),
        (
            "emacs",
            OptionDefinition::new(
                |options| options.emacs_mode,
                |options, value| options.emacs_mode = value
            )
        ),
        (
            "errexit",
            OptionDefinition::new(
                |options| options.exit_on_nonzero_command_exit,
                |options, value| options.exit_on_nonzero_command_exit = value
            )
        ),
        (
            "errtrace",
            OptionDefinition::new(
                |options| options.shell_functions_inherit_err_trap,
                |options, value| options.shell_functions_inherit_err_trap = value
            )
        ),
        (
            "functrace",
            OptionDefinition::new(
                |options| options.shell_functions_inherit_debug_and_return_traps,
                |options, value| options.shell_functions_inherit_debug_and_return_traps = value
            )
        ),
        (
            "hashall",
            OptionDefinition::new(
                |options| options.remember_command_locations,
                |options, value| options.remember_command_locations = value
            )
        ),
        (
            "histexpand",
            OptionDefinition::new(
                |options| options.enable_bang_style_history_substitution,
                |options, value| options.enable_bang_style_history_substitution = value
            )
        ),
        (
            "history",
            OptionDefinition::new(
                |options| options.enable_command_history,
                |options, value| options.enable_command_history = value
            )
        ),
        (
            "ignoreeof",
            OptionDefinition::new(
                |options| options.ignore_eof,
                |options, value| options.ignore_eof = value
            )
        ),
        (
            "interactive-comments",
            OptionDefinition::new(
                |options| options.interactive_comments,
                |options, value| options.interactive_comments = value
            )
        ),
        (
            "keyword",
            OptionDefinition::new(
                |options| options.place_all_assignment_args_in_command_env,
                |options, value| options.place_all_assignment_args_in_command_env = value
            )
        ),
        (
            "monitor",
            OptionDefinition::new(
                |options| options.enable_job_control,
                |options, value| options.enable_job_control = value
            )
        ),
        (
            "noclobber",
            OptionDefinition::new(
                |options| options.disallow_overwriting_regular_files_via_output_redirection,
                |options, value| options
                    .disallow_overwriting_regular_files_via_output_redirection =
                    value
            )
        ),
        (
            "noexec",
            OptionDefinition::new(
                |options| options.do_not_execute_commands,
                |options, value| options.do_not_execute_commands = value
            )
        ),
        (
            "noglob",
            OptionDefinition::new(
                |options| options.disable_filename_globbing,
                |options, value| options.disable_filename_globbing = value
            )
        ),
        ("nolog", OptionDefinition::new(|_| false, |_, _| ())),
        (
            "notify",
            OptionDefinition::new(
                |options| options.notify_job_termination_immediately,
                |options, value| options.notify_job_termination_immediately = value
            )
        ),
        (
            "nounset",
            OptionDefinition::new(
                |options| options.treat_unset_variables_as_error,
                |options, value| options.treat_unset_variables_as_error = value
            )
        ),
        (
            "onecmd",
            OptionDefinition::new(
                |options| options.exit_after_one_command,
                |options, value| options.exit_after_one_command = value
            )
        ),
        (
            "physical",
            OptionDefinition::new(
                |options| options.do_not_resolve_symlinks_when_changing_dir,
                |options, value| options.do_not_resolve_symlinks_when_changing_dir = value
            )
        ),
        (
            "pipefail",
            OptionDefinition::new(
                |options| options.return_first_failure_from_pipeline,
                |options, value| options.return_first_failure_from_pipeline = value
            )
        ),
        (
            "posix",
            OptionDefinition::new(
                |options| options.posix_mode,
                |options, value| options.posix_mode = value
            )
        ),
        (
            "privileged",
            OptionDefinition::new(
                |options| options.real_effective_uid_mismatch,
                |options, value| options.real_effective_uid_mismatch = value
            )
        ),
        (
            "verbose",
            OptionDefinition::new(
                |options| options.print_shell_input_lines,
                |options, value| options.print_shell_input_lines = value
            )
        ),
        (
            "vi",
            OptionDefinition::new(
                |options| options.vi_mode,
                |options, value| options.vi_mode = value
            )
        ),
        (
            "xtrace",
            OptionDefinition::new(
                |options| options.print_commands_and_arguments,
                |options, value| options.print_commands_and_arguments = value
            )
        ),
    ]);
    pub(crate) static ref SHOPT_OPTIONS: HashMap<&'static str, OptionDefinition> = HashMap::from([
        (
            "autocd",
            OptionDefinition::new(
                |options| options.auto_cd,
                |options, value| options.auto_cd = value
            )
        ),
        (
            "assoc_expand_once",
            OptionDefinition::new(
                |options| options.assoc_expand_once,
                |options, value| options.assoc_expand_once = value
            )
        ),
        (
            "cdable_vars",
            OptionDefinition::new(
                |options| options.cdable_vars,
                |options, value| options.cdable_vars = value
            )
        ),
        (
            "cdspell",
            OptionDefinition::new(
                |options| options.cd_autocorrect_spelling,
                |options, value| options.cd_autocorrect_spelling = value
            )
        ),
        (
            "checkhash",
            OptionDefinition::new(
                |options| options.check_hashtable_before_command_exec,
                |options, value| options.check_hashtable_before_command_exec = value
            )
        ),
        (
            "checkjobs",
            OptionDefinition::new(
                |options| options.check_jobs_before_exit,
                |options, value| options.check_jobs_before_exit = value
            )
        ),
        (
            "checkwinsize",
            OptionDefinition::new(
                |options| options.check_window_size_after_external_commands,
                |options, value| options.check_window_size_after_external_commands = value
            )
        ),
        (
            "cmdhist",
            OptionDefinition::new(
                |options| options.save_multiline_cmds_in_history,
                |options, value| options.save_multiline_cmds_in_history = value
            )
        ),
        (
            "compat31",
            OptionDefinition::new(
                |options| options.compat31,
                |options, value| options.compat31 = value
            )
        ),
        (
            "compat32",
            OptionDefinition::new(
                |options| options.compat32,
                |options, value| options.compat32 = value
            )
        ),
        (
            "compat40",
            OptionDefinition::new(
                |options| options.compat40,
                |options, value| options.compat40 = value
            )
        ),
        (
            "compat41",
            OptionDefinition::new(
                |options| options.compat41,
                |options, value| options.compat41 = value
            )
        ),
        (
            "compat42",
            OptionDefinition::new(
                |options| options.compat42,
                |options, value| options.compat42 = value
            )
        ),
        (
            "compat43",
            OptionDefinition::new(
                |options| options.compat43,
                |options, value| options.compat43 = value
            )
        ),
        (
            "compat44",
            OptionDefinition::new(
                |options| options.compat44,
                |options, value| options.compat44 = value
            )
        ),
        (
            "complete_fullquote",
            OptionDefinition::new(
                |options| options.quote_all_metachars_in_completion,
                |options, value| options.quote_all_metachars_in_completion = value
            )
        ),
        (
            "direxpand",
            OptionDefinition::new(
                |options| options.expand_dir_names_on_completion,
                |options, value| options.expand_dir_names_on_completion = value
            )
        ),
        (
            "dirspell",
            OptionDefinition::new(
                |options| options.autocorrect_dir_spelling_on_completion,
                |options, value| options.autocorrect_dir_spelling_on_completion = value
            )
        ),
        (
            "dotglob",
            OptionDefinition::new(
                |options| options.glob_matches_dotfiles,
                |options, value| options.glob_matches_dotfiles = value
            )
        ),
        (
            "execfail",
            OptionDefinition::new(
                |options| options.exit_on_exec_fail,
                |options, value| options.exit_on_exec_fail = value
            )
        ),
        (
            "expand_aliases",
            OptionDefinition::new(
                |options| options.expand_aliases,
                |options, value| options.expand_aliases = value
            )
        ),
        (
            "extdebug",
            OptionDefinition::new(
                |options| options.enable_debugger,
                |options, value| options.enable_debugger = value
            )
        ),
        (
            "extglob",
            OptionDefinition::new(
                |options| options.extended_globbing,
                |options, value| options.extended_globbing = value
            )
        ),
        (
            "extquote",
            OptionDefinition::new(
                |options| options.extquote,
                |options, value| options.extquote = value
            )
        ),
        (
            "failglob",
            OptionDefinition::new(
                |options| options.fail_expansion_on_globs_without_match,
                |options, value| options.fail_expansion_on_globs_without_match = value
            )
        ),
        (
            "force_fignore",
            OptionDefinition::new(
                |options| options.force_fignore,
                |options, value| options.force_fignore = value
            )
        ),
        (
            "globasciiranges",
            OptionDefinition::new(
                |options| options.glob_ranges_use_c_locale,
                |options, value| options.glob_ranges_use_c_locale = value
            )
        ),
        (
            "globstar",
            OptionDefinition::new(
                |options| options.enable_star_star_glob,
                |options, value| options.enable_star_star_glob = value
            )
        ),
        (
            "gnu_errfmt",
            OptionDefinition::new(
                |options| options.errors_in_gnu_format,
                |options, value| options.errors_in_gnu_format = value
            )
        ),
        (
            "histappend",
            OptionDefinition::new(
                |options| options.append_to_history_file,
                |options, value| options.append_to_history_file = value
            )
        ),
        (
            "histreedit",
            OptionDefinition::new(
                |options| options.allow_reedit_failed_history_subst,
                |options, value| options.allow_reedit_failed_history_subst = value
            )
        ),
        (
            "histverify",
            OptionDefinition::new(
                |options| options.allow_modifying_history_substitution,
                |options, value| options.allow_modifying_history_substitution = value
            )
        ),
        (
            "hostcomplete",
            OptionDefinition::new(
                |options| options.enable_hostname_completion,
                |options, value| options.enable_hostname_completion = value
            )
        ),
        (
            "huponexit",
            OptionDefinition::new(
                |options| options.send_sighup_to_all_jobs_on_exit,
                |options, value| options.send_sighup_to_all_jobs_on_exit = value
            )
        ),
        (
            "inherit_errexit",
            OptionDefinition::new(
                |options| options.command_subst_inherits_errexit,
                |options, value| options.command_subst_inherits_errexit = value
            )
        ),
        (
            "interactive_comments",
            OptionDefinition::new(
                |options| options.interactive_comments,
                |options, value| options.interactive_comments = value
            )
        ),
        (
            "lastpipe",
            OptionDefinition::new(
                |options| options.run_last_pipeline_cmd_in_current_shell,
                |options, value| options.run_last_pipeline_cmd_in_current_shell = value
            )
        ),
        (
            "lithist",
            OptionDefinition::new(
                |options| options.embed_newlines_in_multiline_cmds_in_history,
                |options, value| options.embed_newlines_in_multiline_cmds_in_history = value
            )
        ),
        (
            "localvar_inherit",
            OptionDefinition::new(
                |options| options.local_vars_inherit_value_and_attrs,
                |options, value| options.local_vars_inherit_value_and_attrs = value
            )
        ),
        (
            "localvar_unset",
            OptionDefinition::new(
                |options| options.localvar_unset,
                |options, value| options.localvar_unset = value
            )
        ),
        (
            "login_shell",
            OptionDefinition::new(
                |options| options.login_shell,
                |options, value| options.login_shell = value
            )
        ),
        (
            "mailwarn",
            OptionDefinition::new(
                |options| options.mail_warn,
                |options, value| options.mail_warn = value
            )
        ),
        (
            "no_empty_cmd_completion",
            OptionDefinition::new(
                |options| options.no_empty_cmd_completion,
                |options, value| options.no_empty_cmd_completion = value
            )
        ),
        (
            "nocaseglob",
            OptionDefinition::new(
                |options| options.case_insensitive_pathname_expansion,
                |options, value| options.case_insensitive_pathname_expansion = value
            )
        ),
        (
            "nocasematch",
            OptionDefinition::new(
                |options| options.case_insensitive_conditionals,
                |options, value| options.case_insensitive_conditionals = value
            )
        ),
        (
            "nullglob",
            OptionDefinition::new(
                |options| options.expand_non_matching_patterns_to_null,
                |options, value| options.expand_non_matching_patterns_to_null = value
            )
        ),
        (
            "progcomp",
            OptionDefinition::new(
                |options| options.programmable_completion,
                |options, value| options.programmable_completion = value
            )
        ),
        (
            "progcomp_alias",
            OptionDefinition::new(
                |options| options.programmable_completion_alias,
                |options, value| options.programmable_completion_alias = value
            )
        ),
        (
            "promptvars",
            OptionDefinition::new(
                |options| options.expand_prompt_strings,
                |options, value| options.expand_prompt_strings = value
            )
        ),
        (
            "restricted_shell",
            OptionDefinition::new(
                |options| options.restricted_shell,
                |options, value| options.restricted_shell = value
            )
        ),
        (
            "shift_verbose",
            OptionDefinition::new(
                |options| options.shift_verbose,
                |options, value| options.shift_verbose = value
            )
        ),
        (
            "sourcepath",
            OptionDefinition::new(
                |options| options.source_builtin_searches_path,
                |options, value| options.source_builtin_searches_path = value
            )
        ),
        (
            "xpg_echo",
            OptionDefinition::new(
                |options| options.echo_builtin_expands_escape_sequences,
                |options, value| options.echo_builtin_expands_escape_sequences = value
            )
        ),
    ]);
}
