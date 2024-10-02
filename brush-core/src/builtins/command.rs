use clap::Parser;
use std::{fmt::Display, io::Write, path::Path};

use crate::{builtins, commands, error, shell, sys::fs::PathExt, ExecutionResult};

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

    /// Name of command to invoke.
    command_name: String,

    /// Arguments for the built-in.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl builtins::Command for CommandCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        if self.use_default_path {
            return error::unimp("command -p");
        }

        if self.print_description || self.print_verbose_description {
            if let Some(found_cmd) = self.try_find_command(context.shell) {
                if self.print_description {
                    writeln!(context.stdout(), "{found_cmd}")?;
                } else {
                    match found_cmd {
                        FoundCommand::Builtin(_name) => {
                            writeln!(context.stdout(), "{} is a shell builtin", self.command_name)?;
                        }
                        FoundCommand::External(path) => {
                            writeln!(context.stdout(), "{} is {path}", self.command_name)?;
                        }
                    }
                }
                Ok(builtins::ExitCode::Success)
            } else {
                if self.print_verbose_description {
                    writeln!(
                        context.stderr(),
                        "command: {}: not found",
                        self.command_name
                    )?;
                }
                Ok(builtins::ExitCode::Custom(1))
            }
        } else {
            self.execute_command(context).await
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
            FoundCommand::Builtin(name) => write!(f, "{name}"),
            FoundCommand::External(path) => write!(f, "{path}"),
        }
    }
}

impl CommandCommand {
    #[expect(clippy::unwrap_in_result)]
    fn try_find_command(&self, shell: &shell::Shell) -> Option<FoundCommand> {
        // Look in path.
        if self.command_name.contains(std::path::MAIN_SEPARATOR) {
            let candidate_path = shell.get_absolute_path(Path::new(&self.command_name));
            if candidate_path.executable() {
                Some(FoundCommand::External(
                    candidate_path
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                ))
            } else {
                None
            }
        } else {
            if let Some(builtin_cmd) = shell.builtins.get(self.command_name.as_str()) {
                if !builtin_cmd.disabled {
                    return Some(FoundCommand::Builtin(self.command_name.clone()));
                }
            }

            shell
                .find_executables_in_path(self.command_name.as_str())
                .first()
                .map(|path| FoundCommand::External(path.to_string_lossy().to_string()))
        }
    }

    async fn execute_command(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        let args: Vec<_> = std::iter::once(&self.command_name)
            .chain(self.args.iter())
            .map(|arg| arg.into())
            .collect();

        // We can reuse the context, but need to update the name.
        context.command_name.clone_from(&self.command_name);

        // We do not have an existing process group to place this into.
        let mut pgid = None;

        match commands::execute(context, &mut pgid, args, false /* use functions? */).await? {
            commands::CommandSpawnResult::SpawnedProcess(mut child) => {
                // TODO: jobs: review this logic
                let wait_result = child.wait().await?;
                let exec_result = ExecutionResult::from(wait_result);
                Ok(builtins::ExitCode::Custom(exec_result.exit_code))
            }
            commands::CommandSpawnResult::ImmediateExit(code) => {
                Ok(builtins::ExitCode::Custom(code))
            }
            commands::CommandSpawnResult::ExitShell(_)
            | commands::CommandSpawnResult::ReturnFromFunctionOrScript(_)
            | commands::CommandSpawnResult::BreakLoop(_)
            | commands::CommandSpawnResult::ContinueLoop(_) => {
                unreachable!("external command cannot return this spawn result")
            }
        }
    }
}
