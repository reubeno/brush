use crate::CreateOptions;

#[derive(Default, Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct RuntimeOptions {
    //
    // Single-character options.
    //
    /// -a
    pub export_variables_on_modification: bool,
    /// -b
    pub notify_job_termination_immediately: bool,
    /// -e
    pub exit_on_nonzero_command_exit: bool,
    /// -f
    pub disable_filename_globbing: bool,
    /// -h
    pub remember_command_locations: bool,
    /// -k
    pub place_all_assignment_args_in_command_env: bool,
    /// -m
    pub enable_job_control: bool,
    /// -n
    pub do_not_execute_commands: bool,
    /// -p
    pub real_effective_uid_mismatch: bool,
    /// -t
    pub exit_after_one_command: bool,
    /// -u
    pub treat_unset_variables_as_error: bool,
    /// -v
    pub print_shell_input_lines: bool,
    /// -x
    pub print_commands_and_arguments: bool,
    /// -B
    pub perform_brace_expansion: bool,
    /// -C
    pub disallow_overwriting_regular_files_via_output_redirection: bool,
    /// -E
    pub shell_functions_inherit_err_trap: bool,
    /// -H
    pub enable_bang_style_history_substitution: bool,
    /// -P
    pub do_not_resolve_symlinks_when_changing_dir: bool,
    /// -T
    pub shell_functions_inherit_debug_and_return_traps: bool,

    //
    // Options set through -o.
    //
    /// 'emacs'
    pub emacs_mode: bool,
    /// 'history'
    pub enable_command_history: bool,
    /// 'ignoreeof'
    pub ignore_eof: bool,
    /// 'interactive-comments'
    pub allow_comments_in_interactive_commands: bool,
    /// 'pipefail'
    pub return_first_failure_from_pipeline: bool,
    /// 'posix'
    pub posix_mode: bool,
    /// 'vi'
    pub vi_mode: bool,

    //
    // Options set through shopt.
    //
    pub assoc_expand_once: bool,
    pub auto_cd: bool,
    pub cdable_vars: bool,
    pub cd_autocorrect_spelling: bool,
    pub check_hashtable_before_command_exec: bool,
    pub check_jobs_before_exit: bool,
    pub check_window_size_after_external_commands: bool,
    pub save_multiline_cmds_in_history: bool,
    pub compat31: bool,
    pub compat32: bool,
    pub compat40: bool,
    pub compat41: bool,
    pub compat42: bool,
    pub compat43: bool,
    pub compat44: bool,
    pub quote_all_metachars_in_completion: bool,
    pub expand_dir_names_on_completion: bool,
    pub autocorrect_dir_spelling_on_completion: bool,
    pub glob_matches_dotfiles: bool,
    pub exit_on_exec_fail: bool,
    pub expand_aliases: bool,
    pub enable_debugger: bool,
    pub extended_globbing: bool,
    pub extquote: bool,
    pub fail_expansion_on_globs_without_match: bool,
    pub force_fignore: bool,
    pub glob_ranges_use_c_locale: bool,
    pub enable_star_star_glob: bool,
    pub errors_in_gnu_format: bool,
    pub append_to_history_file: bool,
    pub allow_reedit_failed_history_subst: bool,
    pub allow_modifying_history_substitution: bool,
    pub enable_hostname_completion: bool,
    pub send_sighup_to_all_jobs_on_exit: bool,
    pub command_subst_inherits_errexit: bool,
    pub interactive_comments: bool,
    pub run_last_pipeline_cmd_in_current_shell: bool,
    pub embed_newlines_in_multiline_cmds_in_history: bool,
    pub local_vars_inherit_value_and_attrs: bool,
    pub localvar_unset: bool,
    pub login_shell: bool,
    pub mail_warn: bool,
    pub case_insensitive_pathname_expansion: bool,
    pub case_insensitive_conditionals: bool,
    pub no_empty_cmd_completion: bool,
    pub expand_non_matching_patterns_to_null: bool,
    pub programmable_completion: bool,
    pub programmable_completion_alias: bool,
    pub expand_prompt_strings: bool,
    pub restricted_shell: bool,
    pub shift_verbose: bool,
    pub source_builtin_searches_path: bool,
    pub echo_builtin_expands_escape_sequences: bool,

    //
    // Options set by the shell.
    //
    pub interactive: bool,
}

impl RuntimeOptions {
    pub fn defaults_from(create_options: &CreateOptions) -> RuntimeOptions {
        // There's a set of options enabled by default for all shells.
        let mut options = Self {
            interactive: create_options.interactive,
            posix_mode: create_options.posix,
            print_commands_and_arguments: create_options.print_commands_and_arguments,
            print_shell_input_lines: create_options.verbose,
            remember_command_locations: true,
            check_window_size_after_external_commands: true,
            enable_command_history: true,
            extquote: true,
            force_fignore: true,
            enable_hostname_completion: true,
            interactive_comments: true,
            expand_prompt_strings: true,
            source_builtin_searches_path: true,
            ..Self::default()
        };

        // Additional options are enabled by default for interactive shells.
        if create_options.interactive {
            options.enable_bang_style_history_substitution = true;
            options.emacs_mode = !create_options.no_editing;
            options.expand_aliases = true;
        }

        options
    }
}
