use lazy_static::lazy_static;
use std::collections::HashMap;

use crate::Shell;

pub(crate) type OptionGetter = fn(shell: &Shell) -> bool;
pub(crate) type OptionSetter = fn(shell: &mut Shell, value: bool) -> ();

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
                |shell| shell.options.export_variables_on_modification,
                |shell, value| shell.options.export_variables_on_modification = value
            )
        ),
        (
            'b',
            OptionDefinition::new(
                |shell| shell.options.notify_job_termination_immediately,
                |shell, value| shell.options.notify_job_termination_immediately = value
            )
        ),
        (
            'e',
            OptionDefinition::new(
                |shell| shell.options.exit_on_nonzero_command_exit,
                |shell, value| shell.options.exit_on_nonzero_command_exit = value
            )
        ),
        (
            'f',
            OptionDefinition::new(
                |shell| shell.options.disable_filename_globbing,
                |shell, value| shell.options.disable_filename_globbing = value
            )
        ),
        (
            'h',
            OptionDefinition::new(
                |shell| shell.options.remember_command_locations,
                |shell, value| shell.options.remember_command_locations = value
            )
        ),
        (
            'k',
            OptionDefinition::new(
                |shell| shell.options.place_all_assignment_args_in_command_env,
                |shell, value| shell.options.place_all_assignment_args_in_command_env = value
            )
        ),
        (
            'm',
            OptionDefinition::new(
                |shell| shell.options.enable_job_control,
                |shell, value| shell.options.enable_job_control = value
            )
        ),
        (
            'n',
            OptionDefinition::new(
                |shell| shell.options.do_not_execute_commands,
                |shell, value| shell.options.do_not_execute_commands = value
            )
        ),
        (
            'p',
            OptionDefinition::new(
                |shell| shell.options.real_effective_uid_mismatch,
                |shell, value| shell.options.real_effective_uid_mismatch = value
            )
        ),
        (
            't',
            OptionDefinition::new(
                |shell| shell.options.exit_after_one_command,
                |shell, value| shell.options.exit_after_one_command = value
            )
        ),
        (
            'u',
            OptionDefinition::new(
                |shell| shell.options.treat_unset_variables_as_error,
                |shell, value| shell.options.treat_unset_variables_as_error = value
            )
        ),
        (
            'v',
            OptionDefinition::new(
                |shell| shell.options.print_shell_input_lines,
                |shell, value| shell.options.print_shell_input_lines = value
            )
        ),
        (
            'x',
            OptionDefinition::new(
                |shell| shell.options.print_commands_and_arguments,
                |shell, value| shell.options.print_commands_and_arguments = value
            )
        ),
        (
            'B',
            OptionDefinition::new(
                |shell| shell.options.perform_brace_expansion,
                |shell, value| shell.options.perform_brace_expansion = value
            )
        ),
        (
            'C',
            OptionDefinition::new(
                |shell| shell
                    .options
                    .disallow_overwriting_regular_files_via_output_redirection,
                |shell, value| shell
                    .options
                    .disallow_overwriting_regular_files_via_output_redirection =
                    value
            )
        ),
        (
            'E',
            OptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_err_trap,
                |shell, value| shell.options.shell_functions_inherit_err_trap = value
            )
        ),
        (
            'H',
            OptionDefinition::new(
                |shell| shell.options.enable_bang_style_history_substitution,
                |shell, value| shell.options.enable_bang_style_history_substitution = value
            )
        ),
        (
            'P',
            OptionDefinition::new(
                |shell| shell.options.do_not_resolve_symlinks_when_changing_dir,
                |shell, value| shell.options.do_not_resolve_symlinks_when_changing_dir = value
            )
        ),
        (
            'T',
            OptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_debug_and_return_traps,
                |shell, value| shell.options.shell_functions_inherit_debug_and_return_traps = value
            )
        ),
        (
            'i',
            OptionDefinition::new(
                |shell| shell.options.interactive,
                |shell, value| shell.options.interactive = value
            )
        ),
    ]);
    pub(crate) static ref SET_O_OPTIONS: HashMap<&'static str, OptionDefinition> = HashMap::from([
        (
            "allexport",
            OptionDefinition::new(
                |shell| shell.options.export_variables_on_modification,
                |shell, value| shell.options.export_variables_on_modification = value
            )
        ),
        (
            "braceexpand",
            OptionDefinition::new(
                |shell| shell.options.perform_brace_expansion,
                |shell, value| shell.options.perform_brace_expansion = value
            )
        ),
        (
            "emacs",
            OptionDefinition::new(
                |shell| shell.options.emacs_mode,
                |shell, value| shell.options.emacs_mode = value
            )
        ),
        (
            "errexit",
            OptionDefinition::new(
                |shell| shell.options.exit_on_nonzero_command_exit,
                |shell, value| shell.options.exit_on_nonzero_command_exit = value
            )
        ),
        (
            "errtrace",
            OptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_err_trap,
                |shell, value| shell.options.shell_functions_inherit_err_trap = value
            )
        ),
        (
            "functrace",
            OptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_debug_and_return_traps,
                |shell, value| shell.options.shell_functions_inherit_debug_and_return_traps = value
            )
        ),
        (
            "hashall",
            OptionDefinition::new(
                |shell| shell.options.remember_command_locations,
                |shell, value| shell.options.remember_command_locations = value
            )
        ),
        (
            "histexpand",
            OptionDefinition::new(
                |shell| shell.options.enable_bang_style_history_substitution,
                |shell, value| shell.options.enable_bang_style_history_substitution = value
            )
        ),
        (
            "history",
            OptionDefinition::new(
                |shell| shell.options.enable_command_history,
                |shell, value| shell.options.enable_command_history = value
            )
        ),
        (
            "ignoreeof",
            OptionDefinition::new(
                |shell| shell.options.ignore_eof,
                |shell, value| shell.options.ignore_eof = value
            )
        ),
        (
            "interactive-comments",
            OptionDefinition::new(
                |shell| shell.options.allow_comments_in_interactive_commands,
                |shell, value| shell.options.allow_comments_in_interactive_commands = value
            )
        ),
        (
            "keyword",
            OptionDefinition::new(
                |shell| shell.options.place_all_assignment_args_in_command_env,
                |shell, value| shell.options.place_all_assignment_args_in_command_env = value
            )
        ),
        (
            "monitor",
            OptionDefinition::new(
                |shell| shell.options.enable_job_control,
                |shell, value| shell.options.enable_job_control = value
            )
        ),
        (
            "noclobber",
            OptionDefinition::new(
                |shell| shell
                    .options
                    .disallow_overwriting_regular_files_via_output_redirection,
                |shell, value| shell
                    .options
                    .disallow_overwriting_regular_files_via_output_redirection =
                    value
            )
        ),
        (
            "noexec",
            OptionDefinition::new(
                |shell| shell.options.do_not_execute_commands,
                |shell, value| shell.options.do_not_execute_commands = value
            )
        ),
        (
            "noglob",
            OptionDefinition::new(
                |shell| shell.options.disable_filename_globbing,
                |shell, value| shell.options.disable_filename_globbing = value
            )
        ),
        ("nolog", OptionDefinition::new(|_| false, |_, _| ())),
        (
            "notify",
            OptionDefinition::new(
                |shell| shell.options.notify_job_termination_immediately,
                |shell, value| shell.options.notify_job_termination_immediately = value
            )
        ),
        (
            "nounset",
            OptionDefinition::new(
                |shell| shell.options.treat_unset_variables_as_error,
                |shell, value| shell.options.treat_unset_variables_as_error = value
            )
        ),
        (
            "onecmd",
            OptionDefinition::new(
                |shell| shell.options.exit_after_one_command,
                |shell, value| shell.options.exit_after_one_command = value
            )
        ),
        (
            "physical",
            OptionDefinition::new(
                |shell| shell.options.do_not_resolve_symlinks_when_changing_dir,
                |shell, value| shell.options.do_not_resolve_symlinks_when_changing_dir = value
            )
        ),
        (
            "pipefail",
            OptionDefinition::new(
                |shell| shell.options.return_first_failure_from_pipeline,
                |shell, value| shell.options.return_first_failure_from_pipeline = value
            )
        ),
        (
            "posix",
            OptionDefinition::new(
                |shell| shell.options.posix_mode,
                |shell, value| shell.options.posix_mode = value
            )
        ),
        (
            "privileged",
            OptionDefinition::new(
                |shell| shell.options.real_effective_uid_mismatch,
                |shell, value| shell.options.real_effective_uid_mismatch = value
            )
        ),
        (
            "verbose",
            OptionDefinition::new(
                |shell| shell.options.print_shell_input_lines,
                |shell, value| shell.options.print_shell_input_lines = value
            )
        ),
        (
            "vi",
            OptionDefinition::new(
                |shell| shell.options.vi_mode,
                |shell, value| shell.options.vi_mode = value
            )
        ),
        (
            "xtrace",
            OptionDefinition::new(
                |shell| shell.options.print_commands_and_arguments,
                |shell, value| shell.options.print_commands_and_arguments = value
            )
        ),
    ]);
    pub(crate) static ref SHOPT_OPTIONS: HashMap<&'static str, OptionDefinition> = HashMap::from([
        (
            "autocd",
            OptionDefinition::new(
                |shell| shell.options.auto_cd,
                |shell, value| shell.options.auto_cd = value
            )
        ),
        (
            "assoc_expand_once",
            OptionDefinition::new(
                |shell| shell.options.assoc_expand_once,
                |shell, value| shell.options.assoc_expand_once = value
            )
        ),
        (
            "cdable_vars",
            OptionDefinition::new(
                |shell| shell.options.cdable_vars,
                |shell, value| shell.options.cdable_vars = value
            )
        ),
        (
            "cdspell",
            OptionDefinition::new(
                |shell| shell.options.cd_autocorrect_spelling,
                |shell, value| shell.options.cd_autocorrect_spelling = value
            )
        ),
        (
            "checkhash",
            OptionDefinition::new(
                |shell| shell.options.check_hashtable_before_command_exec,
                |shell, value| shell.options.check_hashtable_before_command_exec = value
            )
        ),
        (
            "checkjobs",
            OptionDefinition::new(
                |shell| shell.options.check_jobs_before_exit,
                |shell, value| shell.options.check_jobs_before_exit = value
            )
        ),
        (
            "checkwinsize",
            OptionDefinition::new(
                |shell| shell.options.check_window_size_after_external_commands,
                |shell, value| shell.options.check_window_size_after_external_commands = value
            )
        ),
        (
            "cmdhist",
            OptionDefinition::new(
                |shell| shell.options.save_multiline_cmds_in_history,
                |shell, value| shell.options.save_multiline_cmds_in_history = value
            )
        ),
        (
            "compat31",
            OptionDefinition::new(
                |shell| shell.options.compat31,
                |shell, value| shell.options.compat31 = value
            )
        ),
        (
            "compat32",
            OptionDefinition::new(
                |shell| shell.options.compat32,
                |shell, value| shell.options.compat32 = value
            )
        ),
        (
            "compat40",
            OptionDefinition::new(
                |shell| shell.options.compat40,
                |shell, value| shell.options.compat40 = value
            )
        ),
        (
            "compat41",
            OptionDefinition::new(
                |shell| shell.options.compat41,
                |shell, value| shell.options.compat41 = value
            )
        ),
        (
            "compat42",
            OptionDefinition::new(
                |shell| shell.options.compat42,
                |shell, value| shell.options.compat42 = value
            )
        ),
        (
            "compat43",
            OptionDefinition::new(
                |shell| shell.options.compat43,
                |shell, value| shell.options.compat43 = value
            )
        ),
        (
            "compat44",
            OptionDefinition::new(
                |shell| shell.options.compat44,
                |shell, value| shell.options.compat44 = value
            )
        ),
        (
            "complete_fullquote",
            OptionDefinition::new(
                |shell| shell.options.quote_all_metachars_in_completion,
                |shell, value| shell.options.quote_all_metachars_in_completion = value
            )
        ),
        (
            "direxpand",
            OptionDefinition::new(
                |shell| shell.options.expand_dir_names_on_completion,
                |shell, value| shell.options.expand_dir_names_on_completion = value
            )
        ),
        (
            "dirspell",
            OptionDefinition::new(
                |shell| shell.options.autocorrect_dir_spelling_on_completion,
                |shell, value| shell.options.autocorrect_dir_spelling_on_completion = value
            )
        ),
        (
            "dotglob",
            OptionDefinition::new(
                |shell| shell.options.glob_matches_dotfiles,
                |shell, value| shell.options.glob_matches_dotfiles = value
            )
        ),
        (
            "execfail",
            OptionDefinition::new(
                |shell| shell.options.exit_on_exec_fail,
                |shell, value| shell.options.exit_on_exec_fail = value
            )
        ),
        (
            "expand_aliases",
            OptionDefinition::new(
                |shell| shell.options.expand_aliases,
                |shell, value| shell.options.expand_aliases = value
            )
        ),
        (
            "extdebug",
            OptionDefinition::new(
                |shell| shell.options.enable_debugger,
                |shell, value| shell.options.enable_debugger = value
            )
        ),
        (
            "extglob",
            OptionDefinition::new(
                |shell| shell.options.extended_globbing,
                |shell, value| shell.options.extended_globbing = value
            )
        ),
        (
            "extquote",
            OptionDefinition::new(
                |shell| shell.options.extquote,
                |shell, value| shell.options.extquote = value
            )
        ),
        (
            "failglob",
            OptionDefinition::new(
                |shell| shell.options.fail_expansion_on_globs_without_match,
                |shell, value| shell.options.fail_expansion_on_globs_without_match = value
            )
        ),
        (
            "force_fignore",
            OptionDefinition::new(
                |shell| shell.options.force_fignore,
                |shell, value| shell.options.force_fignore = value
            )
        ),
        (
            "globasciiranges",
            OptionDefinition::new(
                |shell| shell.options.glob_ranges_use_c_locale,
                |shell, value| shell.options.glob_ranges_use_c_locale = value
            )
        ),
        (
            "globstar",
            OptionDefinition::new(
                |shell| shell.options.enable_star_star_glob,
                |shell, value| shell.options.enable_star_star_glob = value
            )
        ),
        (
            "gnu_errfmt",
            OptionDefinition::new(
                |shell| shell.options.errors_in_gnu_format,
                |shell, value| shell.options.errors_in_gnu_format = value
            )
        ),
        (
            "histappend",
            OptionDefinition::new(
                |shell| shell.options.append_to_history_file,
                |shell, value| shell.options.append_to_history_file = value
            )
        ),
        (
            "histreedit",
            OptionDefinition::new(
                |shell| shell.options.allow_reedit_failed_history_subst,
                |shell, value| shell.options.allow_reedit_failed_history_subst = value
            )
        ),
        (
            "histverify",
            OptionDefinition::new(
                |shell| shell.options.allow_modifying_history_substitution,
                |shell, value| shell.options.allow_modifying_history_substitution = value
            )
        ),
        (
            "hostcomplete",
            OptionDefinition::new(
                |shell| shell.options.enable_hostname_completion,
                |shell, value| shell.options.enable_hostname_completion = value
            )
        ),
        (
            "huponexit",
            OptionDefinition::new(
                |shell| shell.options.send_sighup_to_all_jobs_on_exit,
                |shell, value| shell.options.send_sighup_to_all_jobs_on_exit = value
            )
        ),
        (
            "inherit_errexit",
            OptionDefinition::new(
                |shell| shell.options.command_subst_inherits_errexit,
                |shell, value| shell.options.command_subst_inherits_errexit = value
            )
        ),
        (
            "interactive_comments",
            OptionDefinition::new(
                |shell| shell.options.interactive_comments,
                |shell, value| shell.options.interactive_comments = value
            )
        ),
        (
            "lastpipe",
            OptionDefinition::new(
                |shell| shell.options.run_last_pipeline_cmd_in_current_shell,
                |shell, value| shell.options.run_last_pipeline_cmd_in_current_shell = value
            )
        ),
        (
            "lithist",
            OptionDefinition::new(
                |shell| shell.options.embed_newlines_in_multiline_cmds_in_history,
                |shell, value| shell.options.embed_newlines_in_multiline_cmds_in_history = value
            )
        ),
        (
            "localvar_inherit",
            OptionDefinition::new(
                |shell| shell.options.local_vars_inherit_value_and_attrs,
                |shell, value| shell.options.local_vars_inherit_value_and_attrs = value
            )
        ),
        (
            "localvar_unset",
            OptionDefinition::new(
                |shell| shell.options.localvar_unset,
                |shell, value| shell.options.localvar_unset = value
            )
        ),
        (
            "login_shell",
            OptionDefinition::new(
                |shell| shell.options.login_shell,
                |shell, value| shell.options.login_shell = value
            )
        ),
        (
            "mailwarn",
            OptionDefinition::new(
                |shell| shell.options.mail_warn,
                |shell, value| shell.options.mail_warn = value
            )
        ),
        (
            "no_empty_cmd_completion",
            OptionDefinition::new(
                |shell| shell.options.no_empty_cmd_completion,
                |shell, value| shell.options.no_empty_cmd_completion = value
            )
        ),
        (
            "nocaseglob",
            OptionDefinition::new(
                |shell| shell.options.case_insensitive_pathname_expansion,
                |shell, value| shell.options.case_insensitive_pathname_expansion = value
            )
        ),
        (
            "nocasematch",
            OptionDefinition::new(
                |shell| shell.options.case_insensitive_conditionals,
                |shell, value| shell.options.case_insensitive_conditionals = value
            )
        ),
        (
            "nullglob",
            OptionDefinition::new(
                |shell| shell.options.expand_non_matching_patterns_to_null,
                |shell, value| shell.options.expand_non_matching_patterns_to_null = value
            )
        ),
        (
            "progcomp",
            OptionDefinition::new(
                |shell| shell.options.programmable_completion,
                |shell, value| shell.options.programmable_completion = value
            )
        ),
        (
            "progcomp_alias",
            OptionDefinition::new(
                |shell| shell.options.programmable_completion_alias,
                |shell, value| shell.options.programmable_completion_alias = value
            )
        ),
        (
            "promptvars",
            OptionDefinition::new(
                |shell| shell.options.expand_prompt_strings,
                |shell, value| shell.options.expand_prompt_strings = value
            )
        ),
        (
            "restricted_shell",
            OptionDefinition::new(
                |shell| shell.options.restricted_shell,
                |shell, value| shell.options.restricted_shell = value
            )
        ),
        (
            "shift_verbose",
            OptionDefinition::new(
                |shell| shell.options.shift_verbose,
                |shell, value| shell.options.shift_verbose = value
            )
        ),
        (
            "sourcepath",
            OptionDefinition::new(
                |shell| shell.options.source_builtin_searches_path,
                |shell, value| shell.options.source_builtin_searches_path = value
            )
        ),
        (
            "xpg_echo",
            OptionDefinition::new(
                |shell| shell.options.echo_builtin_expands_escape_sequences,
                |shell, value| shell.options.echo_builtin_expands_escape_sequences = value
            )
        ),
    ]);
}
