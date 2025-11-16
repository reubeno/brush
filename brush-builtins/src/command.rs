use clap::Parser;
use std::{fmt::Display, io::Write, path::Path};

use brush_core::{
    ExecutionResult, ExecutionSpawnResult, builtins, commands, pathsearch,
    sys::{self, fs::PathExt},
};

/// Directly invokes an external command, without going through typical search order.
#[derive(Parser)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[arg(short = 'p')]
    use_default_path: bool,

    /// Display a short description of the command.
    #[arg(short = 'v')]
    print_description: bool,

    /// Display a more verbose description of the command.
    #[arg(short = 'V')]
    print_verbose_description: bool,

    /// Command and arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_and_args: Vec<String>,
}

impl CommandCommand {
    fn command(&self) -> Option<&String> {
        self.command_and_args.first()
    }
}

impl builtins::Command for CommandCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Silently exit if no command was provided.
        if let Some(command_name) = self.command() {
            if self.print_description || self.print_verbose_description {
                if let Some(found_cmd) = Self::try_find_command(
                    context.shell,
                    command_name.as_str(),
                    self.use_default_path,
                ) {
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
        shell: &mut brush_core::Shell,
        command_name: &str,
        use_default_path: bool,
    ) -> Option<FoundCommand> {
        // Look in path.
        if command_name.contains(std::path::MAIN_SEPARATOR) {
            let candidate_path = shell.absolute_path(Path::new(command_name));
            if candidate_path.executable() {
                Some(FoundCommand::External(
                    candidate_path.to_string_lossy().to_string(),
                ))
            } else {
                None
            }
        } else {
            if let Some(builtin_cmd) = shell.builtins().get(command_name) {
                if !builtin_cmd.disabled {
                    return Some(FoundCommand::Builtin(command_name.to_owned()));
                }
            }

            if use_default_path {
                let dirs = sys::fs::get_default_standard_utils_paths();

                pathsearch::search_for_executable(dirs.iter().map(String::as_str), command_name)
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
        mut context: brush_core::ExecutionContext<'_>,
        command_name: &str,
        use_default_path: bool,
    ) -> Result<ExecutionResult, brush_core::Error> {
        command_name.clone_into(&mut context.command_name);
        let command_and_args = self.command_and_args.iter().map(|arg| arg.into()).collect();

        let path_dirs = if use_default_path {
            Some(sys::fs::get_default_standard_utils_paths())
        } else {
            None
        };

        // We do not have an existing process group to place this into.
        let mut pgid = None;

        match commands::execute(
            context,
            &mut pgid,
            command_and_args,
            false, /* use functions? */
            path_dirs,
        )
        .await?
        {
            ExecutionSpawnResult::StartedProcess(mut child) => {
                // TODO: jobs: review this logic
                let wait_result = child.wait().await?;
                Ok(ExecutionResult::from(wait_result))
            }
            ExecutionSpawnResult::Completed(result) => Ok(result),
        }
    }
}
