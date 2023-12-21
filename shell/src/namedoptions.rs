use lazy_static::lazy_static;
use std::collections::HashMap;

use crate::Shell;

pub(crate) type ShellOptionGetter = fn(shell: &Shell) -> bool;
pub(crate) type ShellOptionSetter = fn(shell: &mut Shell, value: bool) -> ();

pub(crate) struct ShellOptionDefinition {
    pub getter: ShellOptionGetter,
    pub setter: ShellOptionSetter,
}

impl ShellOptionDefinition {
    fn new(getter: ShellOptionGetter, setter: ShellOptionSetter) -> ShellOptionDefinition {
        ShellOptionDefinition { getter, setter }
    }
}

lazy_static! {
    pub(crate) static ref SET_OPTIONS: HashMap<char, ShellOptionDefinition> = HashMap::from([
        (
            'a',
            ShellOptionDefinition::new(
                |shell| shell.options.export_variables_on_modification,
                |shell, value| shell.options.export_variables_on_modification = value
            )
        ),
        (
            'b',
            ShellOptionDefinition::new(
                |shell| shell.options.notify_job_termination_immediately,
                |shell, value| shell.options.notify_job_termination_immediately = value
            )
        ),
        (
            'e',
            ShellOptionDefinition::new(
                |shell| shell.options.exit_on_nonzero_command_exit,
                |shell, value| shell.options.exit_on_nonzero_command_exit = value
            )
        ),
        (
            'f',
            ShellOptionDefinition::new(
                |shell| shell.options.disable_filename_globbing,
                |shell, value| shell.options.disable_filename_globbing = value
            )
        ),
        (
            'h',
            ShellOptionDefinition::new(
                |shell| shell.options.remember_command_locations,
                |shell, value| shell.options.remember_command_locations = value
            )
        ),
        (
            'k',
            ShellOptionDefinition::new(
                |shell| shell.options.place_all_assignment_args_in_command_env,
                |shell, value| shell.options.place_all_assignment_args_in_command_env = value
            )
        ),
        (
            'm',
            ShellOptionDefinition::new(
                |shell| shell.options.enable_job_control,
                |shell, value| shell.options.enable_job_control = value
            )
        ),
        (
            'n',
            ShellOptionDefinition::new(
                |shell| shell.options.do_not_execute_commands,
                |shell, value| shell.options.do_not_execute_commands = value
            )
        ),
        (
            'p',
            ShellOptionDefinition::new(
                |shell| shell.options.real_effective_uid_mismatch,
                |shell, value| shell.options.real_effective_uid_mismatch = value
            )
        ),
        (
            't',
            ShellOptionDefinition::new(
                |shell| shell.options.exit_after_one_command,
                |shell, value| shell.options.exit_after_one_command = value
            )
        ),
        (
            'u',
            ShellOptionDefinition::new(
                |shell| shell.options.treat_unset_variables_as_error,
                |shell, value| shell.options.treat_unset_variables_as_error = value
            )
        ),
        (
            'v',
            ShellOptionDefinition::new(
                |shell| shell.options.print_shell_input_lines,
                |shell, value| shell.options.print_shell_input_lines = value
            )
        ),
        (
            'x',
            ShellOptionDefinition::new(
                |shell| shell.options.print_commands_and_arguments,
                |shell, value| shell.options.print_commands_and_arguments = value
            )
        ),
        (
            'B',
            ShellOptionDefinition::new(
                |shell| shell.options.perform_brace_expansion,
                |shell, value| shell.options.perform_brace_expansion = value
            )
        ),
        (
            'C',
            ShellOptionDefinition::new(
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
            ShellOptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_err_trap,
                |shell, value| shell.options.shell_functions_inherit_err_trap = value
            )
        ),
        (
            'H',
            ShellOptionDefinition::new(
                |shell| shell.options.enable_bang_style_history_substitution,
                |shell, value| shell.options.enable_bang_style_history_substitution = value
            )
        ),
        (
            'P',
            ShellOptionDefinition::new(
                |shell| shell.options.do_not_resolve_symlinks_when_changing_dir,
                |shell, value| shell.options.do_not_resolve_symlinks_when_changing_dir = value
            )
        ),
        (
            'T',
            ShellOptionDefinition::new(
                |shell| shell.options.shell_functions_inherit_debug_and_return_traps,
                |shell, value| shell.options.shell_functions_inherit_debug_and_return_traps = value
            )
        ),
        (
            'i',
            ShellOptionDefinition::new(
                |shell| shell.options.interactive,
                |shell, value| shell.options.interactive = value
            )
        ),
    ]);
    pub(crate) static ref SET_O_OPTIONS: HashMap<&'static str, ShellOptionDefinition> =
        HashMap::from([
            (
                "allexport",
                ShellOptionDefinition::new(
                    |shell| shell.options.export_variables_on_modification,
                    |shell, value| shell.options.export_variables_on_modification = value
                )
            ),
            (
                "braceexpand",
                ShellOptionDefinition::new(
                    |shell| shell.options.perform_brace_expansion,
                    |shell, value| shell.options.perform_brace_expansion = value
                )
            ),
            (
                "emacs",
                ShellOptionDefinition::new(
                    |shell| shell.options.emacs_mode,
                    |shell, value| shell.options.emacs_mode = value
                )
            ),
            (
                "errexit",
                ShellOptionDefinition::new(
                    |shell| shell.options.exit_on_nonzero_command_exit,
                    |shell, value| shell.options.exit_on_nonzero_command_exit = value
                )
            ),
            (
                "errtrace",
                ShellOptionDefinition::new(
                    |shell| shell.options.shell_functions_inherit_err_trap,
                    |shell, value| shell.options.shell_functions_inherit_err_trap = value
                )
            ),
            (
                "functrace",
                ShellOptionDefinition::new(
                    |shell| shell.options.shell_functions_inherit_debug_and_return_traps,
                    |shell, value| shell.options.shell_functions_inherit_debug_and_return_traps =
                        value
                )
            ),
            (
                "hashall",
                ShellOptionDefinition::new(
                    |shell| shell.options.remember_command_locations,
                    |shell, value| shell.options.remember_command_locations = value
                )
            ),
            (
                "histexpand",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_bang_style_history_substitution,
                    |shell, value| shell.options.enable_bang_style_history_substitution = value
                )
            ),
            (
                "history",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_command_history,
                    |shell, value| shell.options.enable_command_history = value
                )
            ),
            (
                "ignoreeof",
                ShellOptionDefinition::new(
                    |shell| shell.options.ignore_eof,
                    |shell, value| shell.options.ignore_eof = value
                )
            ),
            (
                "interactive-comments",
                ShellOptionDefinition::new(
                    |shell| shell.options.allow_comments_in_interactive_commands,
                    |shell, value| shell.options.allow_comments_in_interactive_commands = value
                )
            ),
            (
                "keyword",
                ShellOptionDefinition::new(
                    |shell| shell.options.place_all_assignment_args_in_command_env,
                    |shell, value| shell.options.place_all_assignment_args_in_command_env = value
                )
            ),
            (
                "monitor",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_job_control,
                    |shell, value| shell.options.enable_job_control = value
                )
            ),
            (
                "noclobber",
                ShellOptionDefinition::new(
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
                ShellOptionDefinition::new(
                    |shell| shell.options.do_not_execute_commands,
                    |shell, value| shell.options.do_not_execute_commands = value
                )
            ),
            (
                "noglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.disable_filename_globbing,
                    |shell, value| shell.options.disable_filename_globbing = value
                )
            ),
            ("nolog", ShellOptionDefinition::new(|_| false, |_, _| ())),
            (
                "notify",
                ShellOptionDefinition::new(
                    |shell| shell.options.notify_job_termination_immediately,
                    |shell, value| shell.options.notify_job_termination_immediately = value
                )
            ),
            (
                "nounset",
                ShellOptionDefinition::new(
                    |shell| shell.options.treat_unset_variables_as_error,
                    |shell, value| shell.options.treat_unset_variables_as_error = value
                )
            ),
            (
                "onecmd",
                ShellOptionDefinition::new(
                    |shell| shell.options.exit_after_one_command,
                    |shell, value| shell.options.exit_after_one_command = value
                )
            ),
            (
                "physical",
                ShellOptionDefinition::new(
                    |shell| shell.options.do_not_resolve_symlinks_when_changing_dir,
                    |shell, value| shell.options.do_not_resolve_symlinks_when_changing_dir = value
                )
            ),
            (
                "pipefail",
                ShellOptionDefinition::new(
                    |shell| shell.options.return_first_failure_from_pipeline,
                    |shell, value| shell.options.return_first_failure_from_pipeline = value
                )
            ),
            (
                "posix",
                ShellOptionDefinition::new(
                    |shell| shell.options.posix_mode,
                    |shell, value| shell.options.posix_mode = value
                )
            ),
            (
                "privileged",
                ShellOptionDefinition::new(
                    |shell| shell.options.real_effective_uid_mismatch,
                    |shell, value| shell.options.real_effective_uid_mismatch = value
                )
            ),
            (
                "verbose",
                ShellOptionDefinition::new(
                    |shell| shell.options.print_shell_input_lines,
                    |shell, value| shell.options.print_shell_input_lines = value
                )
            ),
            (
                "vi",
                ShellOptionDefinition::new(
                    |shell| shell.options.vi_mode,
                    |shell, value| shell.options.vi_mode = value
                )
            ),
            (
                "xtrace",
                ShellOptionDefinition::new(
                    |shell| shell.options.print_commands_and_arguments,
                    |shell, value| shell.options.print_commands_and_arguments = value
                )
            ),
        ]);
    pub(crate) static ref SHOPT_OPTIONS: HashMap<&'static str, ShellOptionDefinition> =
        HashMap::from([
            (
                "autocd",
                ShellOptionDefinition::new(
                    |shell| shell.options.auto_cd,
                    |shell, value| shell.options.auto_cd = value
                )
            ),
            (
                "assoc_expand_once",
                ShellOptionDefinition::new(
                    |shell| shell.options.assoc_expand_once,
                    |shell, value| shell.options.assoc_expand_once = value
                )
            ),
            (
                "cdable_vars",
                ShellOptionDefinition::new(
                    |shell| shell.options.cdable_vars,
                    |shell, value| shell.options.cdable_vars = value
                )
            ),
            (
                "cdspell",
                ShellOptionDefinition::new(
                    |shell| shell.options.cd_autocorrect_spelling,
                    |shell, value| shell.options.cd_autocorrect_spelling = value
                )
            ),
            (
                "checkhash",
                ShellOptionDefinition::new(
                    |shell| shell.options.check_hashtable_before_command_exec,
                    |shell, value| shell.options.check_hashtable_before_command_exec = value
                )
            ),
            (
                "checkjobs",
                ShellOptionDefinition::new(
                    |shell| shell.options.check_jobs_before_exit,
                    |shell, value| shell.options.check_jobs_before_exit = value
                )
            ),
            (
                "checkwinsize",
                ShellOptionDefinition::new(
                    |shell| shell.options.check_window_size_after_external_commands,
                    |shell, value| shell.options.check_window_size_after_external_commands = value
                )
            ),
            (
                "cmdhist",
                ShellOptionDefinition::new(
                    |shell| shell.options.save_multiline_cmds_in_history,
                    |shell, value| shell.options.save_multiline_cmds_in_history = value
                )
            ),
            (
                "compat31",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat31,
                    |shell, value| shell.options.compat31 = value
                )
            ),
            (
                "compat32",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat32,
                    |shell, value| shell.options.compat32 = value
                )
            ),
            (
                "compat40",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat40,
                    |shell, value| shell.options.compat40 = value
                )
            ),
            (
                "compat41",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat41,
                    |shell, value| shell.options.compat41 = value
                )
            ),
            (
                "compat42",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat42,
                    |shell, value| shell.options.compat42 = value
                )
            ),
            (
                "compat43",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat43,
                    |shell, value| shell.options.compat43 = value
                )
            ),
            (
                "compat44",
                ShellOptionDefinition::new(
                    |shell| shell.options.compat44,
                    |shell, value| shell.options.compat44 = value
                )
            ),
            (
                "complete_fullquote",
                ShellOptionDefinition::new(
                    |shell| shell.options.quote_all_metachars_in_completion,
                    |shell, value| shell.options.quote_all_metachars_in_completion = value
                )
            ),
            (
                "direxpand",
                ShellOptionDefinition::new(
                    |shell| shell.options.expand_dir_names_on_completion,
                    |shell, value| shell.options.expand_dir_names_on_completion = value
                )
            ),
            (
                "dirspell",
                ShellOptionDefinition::new(
                    |shell| shell.options.autocorrect_dir_spelling_on_completion,
                    |shell, value| shell.options.autocorrect_dir_spelling_on_completion = value
                )
            ),
            (
                "dotglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.glob_matches_dotfiles,
                    |shell, value| shell.options.glob_matches_dotfiles = value
                )
            ),
            (
                "execfail",
                ShellOptionDefinition::new(
                    |shell| shell.options.exit_on_exec_fail,
                    |shell, value| shell.options.exit_on_exec_fail = value
                )
            ),
            (
                "expand_aliases",
                ShellOptionDefinition::new(
                    |shell| shell.options.expand_aliases,
                    |shell, value| shell.options.expand_aliases = value
                )
            ),
            (
                "extdebug",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_debugger,
                    |shell, value| shell.options.enable_debugger = value
                )
            ),
            (
                "extglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.extended_globbing,
                    |shell, value| shell.options.extended_globbing = value
                )
            ),
            (
                "extquote",
                ShellOptionDefinition::new(
                    |shell| shell.options.extquote,
                    |shell, value| shell.options.extquote = value
                )
            ),
            (
                "failglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.fail_expansion_on_globs_without_match,
                    |shell, value| shell.options.fail_expansion_on_globs_without_match = value
                )
            ),
            (
                "force_fignore",
                ShellOptionDefinition::new(
                    |shell| shell.options.force_fignore,
                    |shell, value| shell.options.force_fignore = value
                )
            ),
            (
                "globasciiranges",
                ShellOptionDefinition::new(
                    |shell| shell.options.glob_ranges_use_c_locale,
                    |shell, value| shell.options.glob_ranges_use_c_locale = value
                )
            ),
            (
                "globstar",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_star_star_glob,
                    |shell, value| shell.options.enable_star_star_glob = value
                )
            ),
            (
                "gnu_errfmt",
                ShellOptionDefinition::new(
                    |shell| shell.options.errors_in_gnu_format,
                    |shell, value| shell.options.errors_in_gnu_format = value
                )
            ),
            (
                "histappend",
                ShellOptionDefinition::new(
                    |shell| shell.options.append_to_history_file,
                    |shell, value| shell.options.append_to_history_file = value
                )
            ),
            (
                "histreedit",
                ShellOptionDefinition::new(
                    |shell| shell.options.allow_reedit_failed_history_subst,
                    |shell, value| shell.options.allow_reedit_failed_history_subst = value
                )
            ),
            (
                "histverify",
                ShellOptionDefinition::new(
                    |shell| shell.options.allow_modifying_history_substitution,
                    |shell, value| shell.options.allow_modifying_history_substitution = value
                )
            ),
            (
                "hostcomplete",
                ShellOptionDefinition::new(
                    |shell| shell.options.enable_hostname_completion,
                    |shell, value| shell.options.enable_hostname_completion = value
                )
            ),
            (
                "huponexit",
                ShellOptionDefinition::new(
                    |shell| shell.options.send_sighup_to_all_jobs_on_exit,
                    |shell, value| shell.options.send_sighup_to_all_jobs_on_exit = value
                )
            ),
            (
                "inherit_errexit",
                ShellOptionDefinition::new(
                    |shell| shell.options.command_subst_inherits_errexit,
                    |shell, value| shell.options.command_subst_inherits_errexit = value
                )
            ),
            (
                "interactive_comments",
                ShellOptionDefinition::new(
                    |shell| shell.options.interactive_comments,
                    |shell, value| shell.options.interactive_comments = value
                )
            ),
            (
                "lastpipe",
                ShellOptionDefinition::new(
                    |shell| shell.options.run_last_pipeline_cmd_in_current_shell,
                    |shell, value| shell.options.run_last_pipeline_cmd_in_current_shell = value
                )
            ),
            (
                "lithist",
                ShellOptionDefinition::new(
                    |shell| shell.options.embed_newlines_in_multiline_cmds_in_history,
                    |shell, value| shell.options.embed_newlines_in_multiline_cmds_in_history =
                        value
                )
            ),
            (
                "localvar_inherit",
                ShellOptionDefinition::new(
                    |shell| shell.options.local_vars_inherit_value_and_attrs,
                    |shell, value| shell.options.local_vars_inherit_value_and_attrs = value
                )
            ),
            (
                "localvar_unset",
                ShellOptionDefinition::new(
                    |shell| shell.options.localvar_unset,
                    |shell, value| shell.options.localvar_unset = value
                )
            ),
            (
                "login_shell",
                ShellOptionDefinition::new(
                    |shell| shell.options.login_shell,
                    |shell, value| shell.options.login_shell = value
                )
            ),
            (
                "mailwarn",
                ShellOptionDefinition::new(
                    |shell| shell.options.mail_warn,
                    |shell, value| shell.options.mail_warn = value
                )
            ),
            (
                "no_empty_cmd_completion",
                ShellOptionDefinition::new(
                    |shell| shell.options.no_empty_cmd_completion,
                    |shell, value| shell.options.no_empty_cmd_completion = value
                )
            ),
            (
                "nocaseglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.case_insensitive_pathname_expansion,
                    |shell, value| shell.options.case_insensitive_pathname_expansion = value
                )
            ),
            (
                "nocasematch",
                ShellOptionDefinition::new(
                    |shell| shell.options.case_insensitive_conditionals,
                    |shell, value| shell.options.case_insensitive_conditionals = value
                )
            ),
            (
                "nullglob",
                ShellOptionDefinition::new(
                    |shell| shell.options.expand_non_matching_patterns_to_null,
                    |shell, value| shell.options.expand_non_matching_patterns_to_null = value
                )
            ),
            (
                "progcomp",
                ShellOptionDefinition::new(
                    |shell| shell.options.programmable_completion,
                    |shell, value| shell.options.programmable_completion = value
                )
            ),
            (
                "progcomp_alias",
                ShellOptionDefinition::new(
                    |shell| shell.options.programmable_completion_alias,
                    |shell, value| shell.options.programmable_completion_alias = value
                )
            ),
            (
                "promptvars",
                ShellOptionDefinition::new(
                    |shell| shell.options.expand_prompt_strings,
                    |shell, value| shell.options.expand_prompt_strings = value
                )
            ),
            (
                "restricted_shell",
                ShellOptionDefinition::new(
                    |shell| shell.options.restricted_shell,
                    |shell, value| shell.options.restricted_shell = value
                )
            ),
            (
                "shift_verbose",
                ShellOptionDefinition::new(
                    |shell| shell.options.shift_verbose,
                    |shell, value| shell.options.shift_verbose = value
                )
            ),
            (
                "sourcepath",
                ShellOptionDefinition::new(
                    |shell| shell.options.source_builtin_searches_path,
                    |shell, value| shell.options.source_builtin_searches_path = value
                )
            ),
            (
                "xpg_echo",
                ShellOptionDefinition::new(
                    |shell| shell.options.echo_builtin_expands_escape_sequences,
                    |shell, value| shell.options.echo_builtin_expands_escape_sequences = value
                )
            ),
        ]);
}
