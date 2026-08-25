use bpaf::Parser;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Write;

use itertools::Itertools;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

/// Tri-state capture of a `set -o`/`+o` style option: absent, present with no
/// value (list all), or present with a value.
pub(crate) struct SetOption {
    enable: Option<Vec<String>>,
    disable: Option<Vec<String>>,
}

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

    let occurrences = bpaf::construct!(flag, value).adjacent().many();

    occurrences.map(|occurrences: Vec<((), Option<String>)>| {
        (!occurrences.is_empty()).then(|| {
            occurrences
                .into_iter()
                .filter_map(|((), opt)| opt)
                .collect::<Vec<_>>()
        })
    })
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

impl builtins::Command for SetCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        let export_variables_on_modification =
            crate::minus_or_plus_flag('a', "+a", "Export variables on modification");
        let notify_job_termination_immediately =
            crate::minus_or_plus_flag('b', "+b", "Notify job termination immediately");
        let exit_on_nonzero_command_exit =
            crate::minus_or_plus_flag('e', "+e", "Exit on nonzero command exit");
        let disable_filename_globbing =
            crate::minus_or_plus_flag('f', "+f", "Disable filename globbing");
        let remember_command_locations =
            crate::minus_or_plus_flag('h', "+h", "Remember command locations");
        let place_all_assignment_args_in_command_env = crate::minus_or_plus_flag(
            'k',
            "+k",
            "Place all assignment args in command environment",
        );
        let enable_job_control = crate::minus_or_plus_flag('m', "+m", "Enable job control");
        let do_not_execute_commands =
            crate::minus_or_plus_flag('n', "+n", "Do not execute commands");
        let real_effective_uid_mismatch =
            crate::minus_or_plus_flag('p', "+p", "Real effective UID mismatch");
        let exit_after_one_command = crate::minus_or_plus_flag('t', "+t", "Exit after one command");
        let treat_unset_variables_as_error =
            crate::minus_or_plus_flag('u', "+u", "Treat unset variables as error");
        let print_shell_input_lines =
            crate::minus_or_plus_flag('v', "+v", "Print shell input lines");
        let print_commands_and_arguments =
            crate::minus_or_plus_flag('x', "+x", "Print commands and arguments");
        let perform_brace_expansion =
            crate::minus_or_plus_flag('B', "+B", "Perform brace expansion");
        let disallow_overwriting_regular_files_via_output_redirection = crate::minus_or_plus_flag(
            'C',
            "+C",
            "Disallow overwriting regular files via output redirection",
        );
        let shell_functions_inherit_err_trap =
            crate::minus_or_plus_flag('E', "+E", "Shell functions inherit ERR trap");
        let enable_bang_style_history_substitution =
            crate::minus_or_plus_flag('H', "+H", "Enable bang style history substitution");
        let do_not_resolve_symlinks_when_changing_dir =
            crate::minus_or_plus_flag('P', "+P", "Do not resolve symlinks when changing dir");
        let shell_functions_inherit_debug_and_return_traps =
            crate::minus_or_plus_flag('T', "+T", "Shell functions inherit DEBUG and RETURN traps");

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
        let double_dash_seen = bpaf::pure(false);

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
            double_dash_seen,
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

    /// Overrides the default [`builtins::Command::new`] flow so that the presence
    /// of a bare `--` terminator can be recorded: the central option-section
    /// splitter drops `--` before bpaf ever sees it, yet `set --` must still
    /// clear the shell's positional parameters.
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

        let os_args: Vec<&OsStr> = options.iter().map(OsStr::new).collect();
        let mut command = Self::parser()
            .to_options()
            .run_inner(os_args.as_slice())
            .map_err(render_parse_failure)?;

        command.set_trailing_args(trailing);
        command.double_dash_seen = double_dash_seen;

        Ok(command)
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        self.positional_args = args;
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

fn render_parse_failure(failure: bpaf::ParseFailure) -> builtins::BuiltinArgParseError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => builtins::BuiltinArgParseError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => builtins::BuiltinArgParseError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => builtins::BuiltinArgParseError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Splits the value-taking `-o` out of a short-option group so that its
/// attached value parses (e.g., `-ov` becomes `-o=v`, `-eo` becomes `-e -o`).
/// bpaf otherwise cannot recognize an attached value on `o` because it is
/// also registered as a plain flag.
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
