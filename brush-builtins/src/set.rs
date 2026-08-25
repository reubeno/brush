use std::collections::HashMap;
use std::io::Write;

use itertools::Itertools;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

/// Tri-state capture of a `set -o`/`+o` style option: absent, present with no
/// value (list all), or present with a value.
#[derive(Default)]
pub(crate) struct SetOption {
    enable: Option<Vec<String>>,
    disable: Option<Vec<String>>,
}

const ID_EXPORT_VARIABLES_ON_MODIFICATION: &str = "export_variables_on_modification";
const ID_NOTIFY_JOB_TERMINATION_IMMEDIATELY: &str = "notify_job_termination_immediately";
const ID_EXIT_ON_NONZERO_COMMAND_EXIT: &str = "exit_on_nonzero_command_exit";
const ID_DISABLE_FILENAME_GLOBBING: &str = "disable_filename_globbing";
const ID_REMEMBER_COMMAND_LOCATIONS: &str = "remember_command_locations";
const ID_PLACE_ALL_ASSIGNMENT_ARGS_IN_COMMAND_ENV: &str =
    "place_all_assignment_args_in_command_env";
const ID_ENABLE_JOB_CONTROL: &str = "enable_job_control";
const ID_DO_NOT_EXECUTE_COMMANDS: &str = "do_not_execute_commands";
const ID_REAL_EFFECTIVE_UID_MISMATCH: &str = "real_effective_uid_mismatch";
const ID_EXIT_AFTER_ONE_COMMAND: &str = "exit_after_one_command";
const ID_TREAT_UNSET_VARIABLES_AS_ERROR: &str = "treat_unset_variables_as_error";
const ID_PRINT_SHELL_INPUT_LINES: &str = "print_shell_input_lines";
const ID_PRINT_COMMANDS_AND_ARGUMENTS: &str = "print_commands_and_arguments";
const ID_PERFORM_BRACE_EXPANSION: &str = "perform_brace_expansion";
const ID_DISALLOW_OVERWRITING_REGULAR_FILES_VIA_OUTPUT_REDIRECTION: &str =
    "disallow_overwriting_regular_files_via_output_redirection";
const ID_SHELL_FUNCTIONS_INHERIT_ERR_TRAP: &str = "shell_functions_inherit_err_trap";
const ID_ENABLE_BANG_STYLE_HISTORY_SUBSTITUTION: &str = "enable_bang_style_history_substitution";
const ID_DO_NOT_RESOLVE_SYMLINKS_WHEN_CHANGING_DIR: &str =
    "do_not_resolve_symlinks_when_changing_dir";
const ID_SHELL_FUNCTIONS_INHERIT_DEBUG_AND_RETURN_TRAPS: &str =
    "shell_functions_inherit_debug_and_return_traps";

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

impl builtins::SpecCommand for SetCommand {
    type Error = brush_core::Error;

    #[expect(clippy::too_many_lines)]
    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        let spec = crate::declare_plus_minus(
            spec,
            'a',
            ID_EXPORT_VARIABLES_ON_MODIFICATION,
            "Export variables on modification",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'b',
            ID_NOTIFY_JOB_TERMINATION_IMMEDIATELY,
            "Notify job termination immediately",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'e',
            ID_EXIT_ON_NONZERO_COMMAND_EXIT,
            "Exit on nonzero command exit",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'f',
            ID_DISABLE_FILENAME_GLOBBING,
            "Disable filename globbing",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'h',
            ID_REMEMBER_COMMAND_LOCATIONS,
            "Remember command locations",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'k',
            ID_PLACE_ALL_ASSIGNMENT_ARGS_IN_COMMAND_ENV,
            "Place all assignment args in command environment",
        );
        let spec =
            crate::declare_plus_minus(spec, 'm', ID_ENABLE_JOB_CONTROL, "Enable job control");
        let spec = crate::declare_plus_minus(
            spec,
            'n',
            ID_DO_NOT_EXECUTE_COMMANDS,
            "Do not execute commands",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'p',
            ID_REAL_EFFECTIVE_UID_MISMATCH,
            "Real effective UID mismatch",
        );
        let spec = crate::declare_plus_minus(
            spec,
            't',
            ID_EXIT_AFTER_ONE_COMMAND,
            "Exit after one command",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'u',
            ID_TREAT_UNSET_VARIABLES_AS_ERROR,
            "Treat unset variables as error",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'v',
            ID_PRINT_SHELL_INPUT_LINES,
            "Print shell input lines",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'x',
            ID_PRINT_COMMANDS_AND_ARGUMENTS,
            "Print commands and arguments",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'B',
            ID_PERFORM_BRACE_EXPANSION,
            "Perform brace expansion",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'C',
            ID_DISALLOW_OVERWRITING_REGULAR_FILES_VIA_OUTPUT_REDIRECTION,
            "Disallow overwriting regular files via output redirection",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'E',
            ID_SHELL_FUNCTIONS_INHERIT_ERR_TRAP,
            "Shell functions inherit ERR trap",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'H',
            ID_ENABLE_BANG_STYLE_HISTORY_SUBSTITUTION,
            "Enable bang style history substitution",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'P',
            ID_DO_NOT_RESOLVE_SYMLINKS_WHEN_CHANGING_DIR,
            "Do not resolve symlinks when changing dir",
        );
        let spec = crate::declare_plus_minus(
            spec,
            'T',
            ID_SHELL_FUNCTIONS_INHERIT_DEBUG_AND_RETURN_TRAPS,
            "Shell functions inherit DEBUG and RETURN traps",
        );

        // N.B. Declared for help rendering; `-o`/`+o` occurrences are
        // extracted from the token stream before the backend parses (see
        // `extract_named_options`).
        spec.hidden_arg(
            "setopt_enable",
            &['o'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("OPT"),
            "Specify a named option; without OPT, lists all named options.",
        )
        .hidden_arg(
            "setopt_disable",
            &[],
            &["+o"],
            builtins::argmodel::ArgKind::Value,
            Some("OPT"),
            "",
        )
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            export_variables_on_modification: crate::read_plus_minus(
                matches,
                ID_EXPORT_VARIABLES_ON_MODIFICATION,
            ),
            notify_job_termination_immediately: crate::read_plus_minus(
                matches,
                ID_NOTIFY_JOB_TERMINATION_IMMEDIATELY,
            ),
            exit_on_nonzero_command_exit: crate::read_plus_minus(
                matches,
                ID_EXIT_ON_NONZERO_COMMAND_EXIT,
            ),
            disable_filename_globbing: crate::read_plus_minus(
                matches,
                ID_DISABLE_FILENAME_GLOBBING,
            ),
            remember_command_locations: crate::read_plus_minus(
                matches,
                ID_REMEMBER_COMMAND_LOCATIONS,
            ),
            place_all_assignment_args_in_command_env: crate::read_plus_minus(
                matches,
                ID_PLACE_ALL_ASSIGNMENT_ARGS_IN_COMMAND_ENV,
            ),
            enable_job_control: crate::read_plus_minus(matches, ID_ENABLE_JOB_CONTROL),
            do_not_execute_commands: crate::read_plus_minus(matches, ID_DO_NOT_EXECUTE_COMMANDS),
            real_effective_uid_mismatch: crate::read_plus_minus(
                matches,
                ID_REAL_EFFECTIVE_UID_MISMATCH,
            ),
            exit_after_one_command: crate::read_plus_minus(matches, ID_EXIT_AFTER_ONE_COMMAND),
            treat_unset_variables_as_error: crate::read_plus_minus(
                matches,
                ID_TREAT_UNSET_VARIABLES_AS_ERROR,
            ),
            print_shell_input_lines: crate::read_plus_minus(matches, ID_PRINT_SHELL_INPUT_LINES),
            print_commands_and_arguments: crate::read_plus_minus(
                matches,
                ID_PRINT_COMMANDS_AND_ARGUMENTS,
            ),
            perform_brace_expansion: crate::read_plus_minus(matches, ID_PERFORM_BRACE_EXPANSION),
            disallow_overwriting_regular_files_via_output_redirection: crate::read_plus_minus(
                matches,
                ID_DISALLOW_OVERWRITING_REGULAR_FILES_VIA_OUTPUT_REDIRECTION,
            ),
            shell_functions_inherit_err_trap: crate::read_plus_minus(
                matches,
                ID_SHELL_FUNCTIONS_INHERIT_ERR_TRAP,
            ),
            enable_bang_style_history_substitution: crate::read_plus_minus(
                matches,
                ID_ENABLE_BANG_STYLE_HISTORY_SUBSTITUTION,
            ),
            do_not_resolve_symlinks_when_changing_dir: crate::read_plus_minus(
                matches,
                ID_DO_NOT_RESOLVE_SYMLINKS_WHEN_CHANGING_DIR,
            ),
            shell_functions_inherit_debug_and_return_traps: crate::read_plus_minus(
                matches,
                ID_SHELL_FUNCTIONS_INHERIT_DEBUG_AND_RETURN_TRAPS,
            ),

            set_option: SetOption::default(),
            positional_args: matches.trailing().to_vec(),
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

        let spec = Self::declare(builtins::argmodel::CommandSpecBuilder::new()).build();
        let mut matches = brush_core::builtins::argmodel::backend().parse(&spec, "", &options)?;

        let mut command = Self::from_matches(&mut matches)?;
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
/// the disable-side arguments declared by [`crate::declare_plus_minus`].
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
