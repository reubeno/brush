use std::io::Write;

use itertools::Itertools;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, variables};

/// Sentinel bound to a bare `-o`/`+o` (a list-all request). `default_missing` injects
/// it for an occurrence without a value; it is not a real option name.
const BARE_OPTION: &str = "\u{0}";

/// Manage set-based shell options.
#[derive(winnow_args::Args)]
#[arg(
    plus_options,
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct SetCommand {
    #[arg(sequence)]
    options: Vec<SetOption>,

    #[arg(positional, double_dash = "preserve", stop_flags)]
    positional_args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(SetCommand, keep_double_dash = positional_args);

/// One option, `true` for its `-` spelling and `false` for its `+` one, in command-line order.
#[derive(winnow_args::Occurrence)]
enum SetOption {
    /// Export variables on modification.
    #[arg(short = 'a', plus = 'a')]
    Allexport(bool),

    /// Notify job termination immediately.
    #[arg(short = 'b', plus = 'b')]
    Notify(bool),

    /// Exit on nonzero command exit.
    #[arg(short = 'e', plus = 'e')]
    Errexit(bool),

    /// Disable filename globbing.
    #[arg(short = 'f', plus = 'f')]
    Noglob(bool),

    /// Remember command locations.
    #[arg(short = 'h', plus = 'h')]
    Hashall(bool),

    /// Place all assignment args in command environment.
    #[arg(short = 'k', plus = 'k')]
    Keyword(bool),

    /// Enable job control.
    #[arg(short = 'm', plus = 'm')]
    Monitor(bool),

    /// Do not execute commands.
    #[arg(short = 'n', plus = 'n')]
    Noexec(bool),

    /// Real effective UID mismatch.
    #[arg(short = 'p', plus = 'p')]
    Privileged(bool),

    /// Exit after one command.
    #[arg(short = 't', plus = 't')]
    Onecmd(bool),

    /// Treat unset variables as error.
    #[arg(short = 'u', plus = 'u')]
    Nounset(bool),

    /// Print shell input lines.
    #[arg(short = 'v', plus = 'v')]
    Verbose(bool),

    /// Print commands and arguments.
    #[arg(short = 'x', plus = 'x')]
    Xtrace(bool),

    /// Perform brace expansion.
    #[arg(short = 'B', plus = 'B')]
    Braceexpand(bool),

    /// Disallow overwriting regular files via output redirection.
    #[arg(short = 'C', plus = 'C')]
    Noclobber(bool),

    /// Shell functions inherit ERR trap.
    #[arg(short = 'E', plus = 'E')]
    Errtrace(bool),

    /// Enable bang style history substitution.
    #[arg(short = 'H', plus = 'H')]
    Histexpand(bool),

    /// Do not resolve symlinks when changing dir.
    #[arg(short = 'P', plus = 'P')]
    Physical(bool),

    /// Shell functions inherit DEBUG and RETURN traps.
    #[arg(short = 'T', plus = 'T')]
    Functrace(bool),

    /// Set (`-o`) or unset (`+o`) the named option; alone, list the options.
    #[arg(
        short = 'o',
        plus = 'o',
        value_name = "OPT",
        default_missing = "\u{0}",
        detached_only
    )]
    Named(bool, String),
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for SetCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();

        for option in &self.options {
            let options = context.shell.options_mut();
            match option {
                SetOption::Allexport(on) => options.export_variables_on_modification = *on,
                SetOption::Notify(on) => options.notify_job_termination_immediately = *on,
                SetOption::Errexit(on) => options.exit_on_nonzero_command_exit = *on,
                SetOption::Noglob(on) => options.disable_filename_globbing = *on,
                SetOption::Hashall(on) => options.remember_command_locations = *on,
                SetOption::Keyword(on) => options.place_all_assignment_args_in_command_env = *on,
                SetOption::Monitor(on) => options.enable_job_control = *on,
                SetOption::Noexec(on) => options.do_not_execute_commands = *on,
                SetOption::Privileged(on) => options.real_effective_uid_mismatch = *on,
                SetOption::Onecmd(on) => options.exit_after_one_command = *on,
                SetOption::Nounset(on) => options.treat_unset_variables_as_error = *on,
                SetOption::Verbose(on) => options.print_shell_input_lines = *on,
                SetOption::Xtrace(on) => options.print_commands_and_arguments = *on,
                SetOption::Braceexpand(on) => options.perform_brace_expansion = *on,
                SetOption::Noclobber(on) => {
                    options.disallow_overwriting_regular_files_via_output_redirection = *on;
                }
                SetOption::Errtrace(on) => options.shell_functions_inherit_err_trap = *on,
                SetOption::Histexpand(on) => options.enable_bang_style_history_substitution = *on,
                SetOption::Physical(on) => options.do_not_resolve_symlinks_when_changing_dir = *on,
                SetOption::Functrace(on) => {
                    options.shell_functions_inherit_debug_and_return_traps = *on;
                }
                SetOption::Named(on, name) if name == BARE_OPTION => {
                    list_named_options(&context, *on, &mut stdout)?;
                }
                SetOption::Named(on, name) => {
                    let named = brush_core::namedoptions::options(
                        brush_core::namedoptions::ShellOptionKind::SetO,
                    );
                    let Some(definition) = named.get(name.as_str()) else {
                        // Like bash, stop at the bad name: what follows is not applied.
                        writeln!(
                            context.stderr(),
                            "{}: {name}: invalid option name",
                            context.command_name
                        )?;
                        return Ok(ExecutionExitCode::InvalidUsage.into());
                    };
                    definition.set(options, *on);
                }
            }
        }

        let args = context.shell.current_shell_args_mut();

        let skip = match self.positional_args.first() {
            Some(x) if x == "-" => {
                if self.positional_args.len() > 1 {
                    args.clear();
                }
                1
            }
            Some(x) if x == "--" => {
                args.clear();
                1
            }
            Some(_) => {
                args.clear();
                0
            }
            None => 0,
        };

        for arg in self.positional_args.iter().skip(skip) {
            args.push(arg.to_owned());
        }

        // A lone `-` ending the options also turns off -x and -v.
        if self.positional_args.first().is_some_and(|x| x == "-") {
            let options = context.shell.options_mut();
            options.print_commands_and_arguments = false;
            options.print_shell_input_lines = false;
        }

        if self.options.is_empty() && self.positional_args.is_empty() {
            stdout.write_all(&display_all(&context)?)?;
        }

        stdout.flush()?;

        Ok(ExecutionResult::success())
    }
}

/// `set -o` (`on`) or `set +o`: every option, as a table or as the commands restoring it.
fn list_named_options(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    on: bool,
    output: &mut impl Write,
) -> Result<(), brush_core::Error> {
    for option in brush_core::namedoptions::options(brush_core::namedoptions::ShellOptionKind::SetO)
        .iter()
        .sorted_by_key(|option| option.name)
    {
        let value = option.definition.get(context.shell.options());
        if on {
            writeln!(
                output,
                "{:15}\t{}",
                option.name,
                if value { "on" } else { "off" }
            )?;
        } else {
            writeln!(
                output,
                "set {} {}",
                if value { "-o" } else { "+o" },
                option.name
            )?;
        }
    }
    Ok(())
}

fn display_all(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<Vec<u8>, brush_core::Error> {
    let mut output = Vec::new();

    for (name, var) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if !var.is_enumerable() {
            continue;
        }

        if matches!(var.value(), variables::ShellValue::Dynamic { .. }) {
            continue;
        }

        if !var.value().is_set() {
            continue;
        }

        writeln!(
            output,
            "{name}={}",
            var.value()
                .format(variables::FormatStyle::Basic, context.shell)?,
        )?;
    }

    if !context.shell.options().posix_mode {
        for (_name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
            writeln!(output, "{}", registration.definition())?;
        }
    }

    Ok(output)
}
