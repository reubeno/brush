use std::{fmt::Display, io::Write, path::Path};

use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, CommandSpec},
    builtins, commands, pathsearch,
    sys::{self, fs::PathExt},
};

/// Directly invokes an external command, without going through typical search order.
#[derive(Default)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    pub use_default_path: bool,

    /// Display a short description of the command.
    pub print_description: bool,

    /// Display a more verbose description of the command.
    pub print_verbose_description: bool,

    /// Command and arguments.
    pub command_and_args: Vec<String>,
}

impl CommandCommand {
    fn command(&self) -> Option<&str> {
        // N.B. A leading `--` ends the builtin's option section; the command
        // name starts after it.
        self.command_and_args
            .iter()
            .position(|s| s != "--")
            .map(|ix| self.command_and_args[ix].as_str())
    }
}

const ID_USE_DEFAULT_PATH: &str = "use_default_path";
const ID_PRINT_DESCRIPTION: &str = "print_description";
const ID_PRINT_VERBOSE_DESCRIPTION: &str = "print_verbose_description";

impl builtins::SpecCommand for CommandCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            args: &[
                ArgSpec::flag(ID_USE_DEFAULT_PATH, &['p'], &[], "Use default PATH value."),
                ArgSpec::flag(
                    ID_PRINT_DESCRIPTION,
                    &['v'],
                    &[],
                    "Display a short description of the command.",
                ),
                ArgSpec::flag(
                    ID_PRINT_VERBOSE_DESCRIPTION,
                    &['V'],
                    &[],
                    "Display a more verbose description of the command.",
                ),
            ],
            positionals: &[],
        };
        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            use_default_path: values.flag(ID_USE_DEFAULT_PATH),
            print_description: values.flag(ID_PRINT_DESCRIPTION),
            print_verbose_description: values.flag(ID_PRINT_VERBOSE_DESCRIPTION),
            command_and_args: values.trailing().to_vec(),
        })
    }

    fn about() -> &'static str {
        "Directly invokes an external command, without going through typical search order."
    }

    fn synopsis() -> &'static str {
        "[-pvV] [COMMAND [ARG]...]"
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Silently exit if no command was provided.
        if let Some(command_name) = self.command() {
            if self.print_description || self.print_verbose_description {
                if let Some(found_cmd) =
                    Self::try_find_command(context.shell, command_name, self.use_default_path)
                {
                    if self.print_description {
                        writeln!(context.stdout(), "{found_cmd}")?;
                    } else {
                        match found_cmd {
                            FoundCommand::Builtin(_name) => {
                                writeln!(context.stdout(), "{command_name} is a shell builtin")?;
                            }
                            FoundCommand::External(path) => {
                                writeln!(context.stdout(), "{command_name} is {path}")?;
                            }
                        }
                    }
                    Ok(ExecutionResult::success())
                } else {
                    if self.print_verbose_description {
                        writeln!(context.stderr(), "command: {command_name}: not found")?;
                    }
                    Ok(ExecutionResult::general_error())
                }
            } else {
                self.execute_command(context, command_name, self.use_default_path)
                    .await
            }
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

enum FoundCommand {
    Builtin(String),
    External(String),
}

impl Display for FoundCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(name) => write!(f, "{name}"),
            Self::External(path) => write!(f, "{path}"),
        }
    }
}

impl CommandCommand {
    fn try_find_command(
        shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
        command_name: &str,
        use_default_path: bool,
    ) -> Option<FoundCommand> {
        // Look in path.
        if sys::fs::contains_path_separator(command_name) {
            let candidate_path = shell.absolute_path(Path::new(command_name));
            if candidate_path.executable() {
                Some(FoundCommand::External(
                    candidate_path.to_string_lossy().to_string(),
                ))
            } else {
                None
            }
        } else {
            if let Some(builtin_cmd) = shell.builtins().get(command_name)
                && !builtin_cmd.disabled
            {
                return Some(FoundCommand::Builtin(command_name.to_owned()));
            }

            if use_default_path {
                let dirs = sys::fs::get_default_standard_utils_paths();

                pathsearch::search_for_executable(dirs.iter(), command_name)
                    .next()
                    .map(|path| FoundCommand::External(path.to_string_lossy().to_string()))
            } else {
                shell
                    .find_first_executable_in_path_using_cache(command_name)
                    .map(|path| FoundCommand::External(path.to_string_lossy().to_string()))
            }
        }
    }

    async fn execute_command(
        &self,
        mut context: brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        command_name: &str,
        use_default_path: bool,
    ) -> Result<ExecutionResult, brush_core::Error> {
        command_name.clone_into(&mut context.command_name);

        // N.B. The spawned-command machinery expects the command name itself
        // as the first element; leading `--` markers are skipped so it lands
        // there.
        let name_ix = self
            .command_and_args
            .iter()
            .position(|s| s.as_str() == command_name)
            .unwrap_or(0);
        let command_and_args = self.command_and_args[name_ix.min(self.command_and_args.len())..]
            .iter()
            .map(brush_core::CommandArg::from);

        let path_dirs = if use_default_path {
            Some(sys::fs::get_default_standard_utils_paths())
        } else {
            None
        };

        let mut cmd = commands::SimpleCommand::new(
            commands::ShellForCommand::ParentShell(context.shell),
            context.params,
            context.command_name,
            command_and_args,
        );
        cmd.use_functions = false;
        cmd.path_dirs = path_dirs;

        let spawn_result = cmd.execute().await?;
        let wait_result = spawn_result.wait().await?;

        Ok(wait_result.into())
    }
}
