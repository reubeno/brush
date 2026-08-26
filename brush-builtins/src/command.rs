//! The `command` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(CommandCommand);

use brush_core::{
    ExecutionResult, commands, pathsearch,
    sys::{self, fs::PathExt},
};
use std::{fmt::Display, io::Write, path::Path};

impl CommandCommand {}

pub(super) enum FoundCommand {
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
        let command_and_args = self
            .command_and_args
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

async fn execute<SE: brush_core::ShellExtensions>(
    command: &CommandCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // Silently exit if no command was provided.
    if let Some(command_name) = command.command_and_args.first().map(|s| s.as_str()) {
        if command.print_description || command.print_verbose_description {
            if let Some(found_cmd) = CommandCommand::try_find_command(
                context.shell,
                command_name,
                command.use_default_path,
            ) {
                if command.print_description {
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
                if command.print_verbose_description {
                    writeln!(context.stderr(), "command: {command_name}: not found")?;
                }
                Ok(ExecutionResult::general_error())
            }
        } else {
            command
                .execute_command(context, command_name, command.use_default_path)
                .await
        }
    } else {
        Ok(ExecutionResult::success())
    }
}
