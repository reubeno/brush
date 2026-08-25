use std::collections::HashMap;
use std::io::Write;

use itertools::Itertools;

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

/// Tri-state capture of a `set -o`/`+o` style option: absent, present with no
/// value (list all), or present with a value.
#[derive(Default)]
pub(crate) struct SetOption {
    enable: Option<Vec<String>>,
    disable: Option<Vec<String>>,
}

/// Manage set-based shell options.
pub(crate) struct SetCommand {
    export_variables_on_modification: Option<bool>,
    notify_job_termination_immediately: Option<bool>,
    exit_on_nonzero_command_exit: Option<bool>,
    disable_filename_globbing: Option<bool>,
    remember_command_locations: Option<bool>,
    place_all_assignment_args_in_command_env: Option<bool>,
    enable_job_control: Option<bool>,
    do_not_execute_commands: Option<bool>,
    real_effective_uid_mismatch: Option<bool>,
    exit_after_one_command: Option<bool>,
    treat_unset_variables_as_error: Option<bool>,
    print_shell_input_lines: Option<bool>,
    print_commands_and_arguments: Option<bool>,
    perform_brace_expansion: Option<bool>,
    disallow_overwriting_regular_files_via_output_redirection: Option<bool>,
    shell_functions_inherit_err_trap: Option<bool>,
    enable_bang_style_history_substitution: Option<bool>,
    do_not_resolve_symlinks_when_changing_dir: Option<bool>,
    shell_functions_inherit_debug_and_return_traps: Option<bool>,

    set_option: SetOption,
    positional_args: Vec<String>,
    double_dash_seen: bool,
}

static SET_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(
            "set_a_enable",
            &['a'],
            &[],
            "Export variables on modification",
        ),
        ArgSpec::hidden_flag("set_a_disable", &[], &["+a"], ""),
        ArgSpec::flag(
            "set_b_enable",
            &['b'],
            &[],
            "Notify job termination immediately",
        ),
        ArgSpec::hidden_flag("set_b_disable", &[], &["+b"], ""),
        ArgSpec::flag("set_e_enable", &['e'], &[], "Exit on nonzero command exit"),
        ArgSpec::hidden_flag("set_e_disable", &[], &["+e"], ""),
        ArgSpec::flag("set_f_enable", &['f'], &[], "Disable filename globbing"),
        ArgSpec::hidden_flag("set_f_disable", &[], &["+f"], ""),
        ArgSpec::flag("set_h_enable", &['h'], &[], "Remember command locations"),
        ArgSpec::hidden_flag("set_h_disable", &[], &["+h"], ""),
        ArgSpec::flag(
            "set_k_enable",
            &['k'],
            &[],
            "Place all assignment args in command environment",
        ),
        ArgSpec::hidden_flag("set_k_disable", &[], &["+k"], ""),
        ArgSpec::flag("set_m_enable", &['m'], &[], "Enable job control"),
        ArgSpec::hidden_flag("set_m_disable", &[], &["+m"], ""),
        ArgSpec::flag("set_n_enable", &['n'], &[], "Do not execute commands"),
        ArgSpec::hidden_flag("set_n_disable", &[], &["+n"], ""),
        ArgSpec::flag("set_p_enable", &['p'], &[], "Real effective UID mismatch"),
        ArgSpec::hidden_flag("set_p_disable", &[], &["+p"], ""),
        ArgSpec::flag("set_t_enable", &['t'], &[], "Exit after one command"),
        ArgSpec::hidden_flag("set_t_disable", &[], &["+t"], ""),
        ArgSpec::flag(
            "set_u_enable",
            &['u'],
            &[],
            "Treat unset variables as error",
        ),
        ArgSpec::hidden_flag("set_u_disable", &[], &["+u"], ""),
        ArgSpec::flag("set_v_enable", &['v'], &[], "Print shell input lines"),
        ArgSpec::hidden_flag("set_v_disable", &[], &["+v"], ""),
        ArgSpec::flag("set_x_enable", &['x'], &[], "Print commands and arguments"),
        ArgSpec::hidden_flag("set_x_disable", &[], &["+x"], ""),
        ArgSpec::flag("set_B_enable", &['B'], &[], "Perform brace expansion"),
        ArgSpec::hidden_flag("set_B_disable", &[], &["+B"], ""),
        ArgSpec::flag(
            "set_C_enable",
            &['C'],
            &[],
            "Disallow overwriting regular files via output redirection",
        ),
        ArgSpec::hidden_flag("set_C_disable", &[], &["+C"], ""),
        ArgSpec::flag(
            "set_E_enable",
            &['E'],
            &[],
            "Shell functions inherit ERR trap",
        ),
        ArgSpec::hidden_flag("set_E_disable", &[], &["+E"], ""),
        ArgSpec::flag(
            "set_H_enable",
            &['H'],
            &[],
            "Enable bang style history substitution",
        ),
        ArgSpec::hidden_flag("set_H_disable", &[], &["+H"], ""),
        ArgSpec::flag(
            "set_P_enable",
            &['P'],
            &[],
            "Do not resolve symlinks when changing dir",
        ),
        ArgSpec::hidden_flag("set_P_disable", &[], &["+P"], ""),
        ArgSpec::flag(
            "set_T_enable",
            &['T'],
            &[],
            "Shell functions inherit DEBUG and RETURN traps",
        ),
        ArgSpec::hidden_flag("set_T_disable", &[], &["+T"], ""),
        // N.B. Declared for help rendering; `-o`/`+o` occurrences are
        // extracted from the token stream before the backend parses (see
        // `extract_named_options`).
        ArgSpec::hidden_value(
            "setopt_enable",
            &['o'],
            &[],
            "OPT",
            "Specify a named option; without OPT, lists all named options.",
        ),
        ArgSpec::hidden_value("setopt_disable", &[], &["+o"], "OPT", ""),
    ],
    positionals: &[],
};

impl builtins::SpecCommand for SetCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &SET_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            export_variables_on_modification: crate::read_plus_minus(
                values,
                "set_a_enable",
                "set_a_disable",
            ),
            notify_job_termination_immediately: crate::read_plus_minus(
                values,
                "set_b_enable",
                "set_b_disable",
            ),
            exit_on_nonzero_command_exit: crate::read_plus_minus(
                values,
                "set_e_enable",
                "set_e_disable",
            ),
            disable_filename_globbing: crate::read_plus_minus(
                values,
                "set_f_enable",
                "set_f_disable",
            ),
            remember_command_locations: crate::read_plus_minus(
                values,
                "set_h_enable",
                "set_h_disable",
            ),
            place_all_assignment_args_in_command_env: crate::read_plus_minus(
                values,
                "set_k_enable",
                "set_k_disable",
            ),
            enable_job_control: crate::read_plus_minus(values, "set_m_enable", "set_m_disable"),
            do_not_execute_commands: crate::read_plus_minus(
                values,
                "set_n_enable",
                "set_n_disable",
            ),
            real_effective_uid_mismatch: crate::read_plus_minus(
                values,
                "set_p_enable",
                "set_p_disable",
            ),
            exit_after_one_command: crate::read_plus_minus(values, "set_t_enable", "set_t_disable"),
            treat_unset_variables_as_error: crate::read_plus_minus(
                values,
                "set_u_enable",
                "set_u_disable",
            ),
            print_shell_input_lines: crate::read_plus_minus(
                values,
                "set_v_enable",
                "set_v_disable",
            ),
            print_commands_and_arguments: crate::read_plus_minus(
                values,
                "set_x_enable",
                "set_x_disable",
            ),
            perform_brace_expansion: crate::read_plus_minus(
                values,
                "set_B_enable",
                "set_B_disable",
            ),
            disallow_overwriting_regular_files_via_output_redirection: crate::read_plus_minus(
                values,
                "set_C_enable",
                "set_C_disable",
            ),
            shell_functions_inherit_err_trap: crate::read_plus_minus(
                values,
                "set_E_enable",
                "set_E_disable",
            ),
            enable_bang_style_history_substitution: crate::read_plus_minus(
                values,
                "set_H_enable",
                "set_H_disable",
            ),
            do_not_resolve_symlinks_when_changing_dir: crate::read_plus_minus(
                values,
                "set_P_enable",
                "set_P_disable",
            ),
            shell_functions_inherit_debug_and_return_traps: crate::read_plus_minus(
                values,
                "set_T_enable",
                "set_T_disable",
            ),

            set_option: SetOption::default(),
            positional_args: values.trailing().to_vec(),
            double_dash_seen: false,
        })
    }

    fn about() -> &'static str {
        "Manage set-based shell options and positional parameters."
    }

    fn synopsis() -> &'static str {
        "[-efhkmnpstuvxBCHEPT] [-o OPT] [+OPT] [--] [ARGS]..."
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

    /// Overrides the default [`builtins::SpecCommand::new`] flow so that the presence
    /// of a bare `--` terminator can be recorded: the central option-section
    /// splitter drops `--` before the backend ever sees it, yet `set --` must
    /// still clear the shell's positional parameters.
    ///
    /// It additionally expands `+`-style option groups and `-o` short-option
    /// groups, extracts the `-o`/`+o` tri-state occurrences, and rewrites
    /// remaining `+x` spellings into forms the argument backend can match.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let double_dash_seen = args.iter().any(|arg| arg == "--");

        // Mirror the central flow: expand '+'-style option groups, split off the
        // trailing section of verbatim operands, then parse the leading options.
        let mut expanded = Vec::with_capacity(args.len());
        for arg in args {
            match arg.strip_prefix('+').filter(|group| !group.is_empty()) {
                Some(group) if !group.starts_with('+') && !group.contains('=') => {
                    expanded.extend(group.chars().map(|c| format!("+{c}")));
                }
                _ => expanded.extend(expand_dash_o_group(&arg)),
            }
        }

        let (options, trailing) =
            builtins::split_option_section(&expanded, Self::value_taking_short_options(), &[]);

        let (mut options, enable, disable) = extract_named_options(options);

        // N.B. Rewrite `+x`-style spellings into the corresponding hidden long
        // forms that the argument backend can match; this happens *after*
        // splitting because the splitter classifies `--+x` as an operand.
        rewrite_plus_flags(&mut options);

        let mut values =
            brush_core::builtins::argmodel::backend().parse(Self::spec(), "", &options)?;

        let mut command = Self::from_matches(&mut values)?;
        command.set_option = SetOption { enable, disable };
        command.positional_args = trailing;
        command.double_dash_seen = double_dash_seen;

        Ok(command)
    }

    #[expect(clippy::too_many_lines)]
    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        #[expect(
            clippy::useless_let_if_seq,
            reason = "each option block conditionally marks that an option was seen"
        )]
        let mut saw_option = false;

        if self.print_commands_and_arguments.is_some() {
            context.shell.options_mut().print_commands_and_arguments =
                self.print_commands_and_arguments.unwrap_or_default();
            saw_option = true;
        }

        if let Some(value) = self.export_variables_on_modification {
            context.shell.options_mut().export_variables_on_modification = value;
            saw_option = true;
        }

        if let Some(value) = self.notify_job_termination_immediately {
            context
                .shell
                .options_mut()
                .notify_job_termination_immediately = value;
            saw_option = true;
        }

        if let Some(value) = self.exit_on_nonzero_command_exit {
            context.shell.options_mut().exit_on_nonzero_command_exit = value;
            saw_option = true;
        }

        if let Some(value) = self.disable_filename_globbing {
            context.shell.options_mut().disable_filename_globbing = value;
            saw_option = true;
        }

        if let Some(value) = self.remember_command_locations {
            context.shell.options_mut().remember_command_locations = value;
            saw_option = true;
        }

        if let Some(value) = self.place_all_assignment_args_in_command_env {
            context
                .shell
                .options_mut()
                .place_all_assignment_args_in_command_env = value;
            saw_option = true;
        }

        if let Some(value) = self.enable_job_control {
            context.shell.options_mut().enable_job_control = value;
            saw_option = true;
        }

        if let Some(value) = self.do_not_execute_commands {
            context.shell.options_mut().do_not_execute_commands = value;
            saw_option = true;
        }

        if let Some(value) = self.real_effective_uid_mismatch {
            context.shell.options_mut().real_effective_uid_mismatch = value;
            saw_option = true;
        }

        if let Some(value) = self.exit_after_one_command {
            context.shell.options_mut().exit_after_one_command = value;
            saw_option = true;
        }

        if let Some(value) = self.treat_unset_variables_as_error {
            context.shell.options_mut().treat_unset_variables_as_error = value;
            saw_option = true;
        }

        if let Some(value) = self.print_shell_input_lines {
            context.shell.options_mut().print_shell_input_lines = value;
            saw_option = true;
        }

        if let Some(value) = self.perform_brace_expansion {
            context.shell.options_mut().perform_brace_expansion = value;
            saw_option = true;
        }

        if let Some(value) = self.disallow_overwriting_regular_files_via_output_redirection {
            context
                .shell
                .options_mut()
                .disallow_overwriting_regular_files_via_output_redirection = value;
            saw_option = true;
        }

        if let Some(value) = self.shell_functions_inherit_err_trap {
            context.shell.options_mut().shell_functions_inherit_err_trap = value;
            saw_option = true;
        }

        if let Some(value) = self.enable_bang_style_history_substitution {
            context
                .shell
                .options_mut()
                .enable_bang_style_history_substitution = value;
            saw_option = true;
        }

        if let Some(value) = self.do_not_resolve_symlinks_when_changing_dir {
            context
                .shell
                .options_mut()
                .do_not_resolve_symlinks_when_changing_dir = value;
            saw_option = true;
        }

        if let Some(value) = self.shell_functions_inherit_debug_and_return_traps {
            context
                .shell
                .options_mut()
                .shell_functions_inherit_debug_and_return_traps = value;
            saw_option = true;
        }

        let mut named_options: HashMap<String, bool> = HashMap::new();
        if let Some(option_names) = &self.set_option.disable {
            saw_option = true;
            if option_names.is_empty() {
                for option in brush_core::namedoptions::options(
                    brush_core::namedoptions::ShellOptionKind::SetO,
                )
                .iter()
                .sorted_by_key(|option| option.name)
                {
                    let option_value = option.definition.get(context.shell.options());
                    let option_value_str = if option_value { "-o" } else { "+o" };
                    writeln!(context.stdout(), "set {option_value_str} {}", option.name)?;
                }
            } else {
                for option_name in option_names {
                    named_options.insert(option_name.to_owned(), false);
                }
            }
        }
        if let Some(option_names) = &self.set_option.enable {
            saw_option = true;
            if option_names.is_empty() {
                for option in brush_core::namedoptions::options(
                    brush_core::namedoptions::ShellOptionKind::SetO,
                )
                .iter()
                .sorted_by_key(|option| option.name)
                {
                    let option_value = option.definition.get(context.shell.options());
                    let option_value_str = if option_value { "on" } else { "off" };
                    writeln!(context.stdout(), "{:15}\t{option_value_str}", option.name)?;
                }
            } else {
                for option_name in option_names {
                    named_options.insert(option_name.to_owned(), true);
                }
            }
        }

        for (option_name, value) in named_options {
            if let Some(option_def) =
                brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
                    .get(option_name.as_str())
            {
                option_def.set(context.shell.options_mut(), value);
            } else {
                result = ExecutionExitCode::InvalidUsage.into();
            }
        }

        let args = context.shell.current_shell_args_mut();

        // N.B. A leading `--` in the captured operands acts as an option
        // terminator and is not part of the positional parameters.
        let positional_args: &[String] =
            if self.positional_args.first().map(String::as_str) == Some("--") {
                &self.positional_args[1..]
            } else {
                &self.positional_args
            };

        let skip = match positional_args.first() {
            Some(x) if x == "-" => {
                if positional_args.len() > 1 {
                    args.clear();
                }
                1
            }
            Some(_) => {
                args.clear();
                0
            }
            None => {
                if self.double_dash_seen {
                    args.clear();
                }
                0
            }
        };

        for arg in positional_args.iter().skip(skip) {
            args.push(arg.to_owned());
        }

        // N.B. A bare `--` counts as an operation on the positional parameters
        // rather than as a request to display them.
        saw_option = saw_option || !positional_args.is_empty() || self.double_dash_seen;

        // If we *still* haven't seen any options, then we need to display all variables and
        // functions.
        if !saw_option {
            display_all(&context)?;
        }

        Ok(result)
    }
}

/// Extracts `-o`/`+o` occurrences from the option section, returning the
/// remaining tokens along with the enable/disable values.
///
/// Mirrors the historical parser's tri-state semantics: an option absent
/// entirely maps to `None`; present occurrences accumulate any provided
/// values; a present occurrence with no value yields an empty vector, which
/// means "list all named options".
fn extract_named_options(
    options: Vec<String>,
) -> (Vec<String>, Option<Vec<String>>, Option<Vec<String>>) {
    let mut rest = Vec::with_capacity(options.len());
    let mut enable: Option<Vec<String>> = None;
    let mut disable: Option<Vec<String>> = None;
    let mut iter = options.into_iter().peekable();

    while let Some(arg) = iter.next() {
        if arg == "-o" || arg == "+o" {
            // Consume a following word as the named option value unless it
            // looks like another option itself.
            let value = match iter.peek() {
                Some(next) if !next.starts_with('-') && !next.starts_with('+') => iter.next(),
                _ => None,
            };

            let slot = if arg == "-o" {
                &mut enable
            } else {
                &mut disable
            };
            let slot = slot.get_or_insert_with(Vec::new);
            if let Some(value) = value {
                slot.push(value);
            }
        } else if let Some(value) = arg.strip_prefix("-o=") {
            enable.get_or_insert_with(Vec::new).push(value.to_owned());
        } else {
            rest.push(arg);
        }
    }

    (rest, enable, disable)
}

/// Rewrites `+x`-style tokens into the corresponding hidden long spellings
/// (e.g., `+x` becomes `--+x`) that the argument backend can match against
/// the disable-side arguments in this command's spec.
fn rewrite_plus_flags(options: &mut [String]) {
    for arg in options.iter_mut() {
        if let Some(group) = arg.strip_prefix('+').filter(|g| !g.is_empty()) {
            if !group.starts_with('+') && !group.contains('=') {
                *arg = format!("--+{group}");
            }
        }
    }
}

/// Splits the value-taking `-o` out of a short-option group so that its
/// attached value parses (e.g., `-ov` becomes `-o=v`, `-eo` becomes `-e -o`).
fn expand_dash_o_group(arg: &str) -> Vec<String> {
    let Some(group) = arg
        .strip_prefix('-')
        .filter(|group| !group.is_empty() && !group.starts_with('-') && !group.contains('='))
    else {
        return vec![arg.to_owned()];
    };

    // Split the group at the `-o` option character, if present; any trailing
    // characters form an attached option value.
    match group.split_once('o') {
        None => vec![arg.to_owned()],
        Some((head, tail)) => {
            let mut expanded = Vec::with_capacity(3);
            if !head.is_empty() {
                expanded.push(format!("-{head}"));
            }

            if tail.is_empty() {
                expanded.push(String::from("-o"));
            } else {
                expanded.push(format!("-o={tail}"));
            }

            expanded
        }
    }
}

fn display_all(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<(), brush_core::Error> {
    // Display variables.
    for (name, var) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if !var.is_enumerable() {
            continue;
        }

        // TODO(set): For now, skip all dynamic variables. The current behavior
        // of bash is not quite clear. We've empirically found that some
        // special variables don't get displayed until they're observed
        // at least once.
        if matches!(var.value(), variables::ShellValue::Dynamic { .. }) {
            continue;
        }

        // Skip variables that have been declared but are unset.
        if !var.value().is_set() {
            continue;
        }

        writeln!(
            context.stdout(),
            "{name}={}",
            var.value()
                .format(variables::FormatStyle::Basic, context.shell)?,
        )?;
    }

    // Display functions... unless we're in posix compliance mode.
    if !context.shell.options().posix_mode {
        for (_name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
            writeln!(context.stdout(), "{}", registration.definition())?;
        }
    }

    Ok(())
}
