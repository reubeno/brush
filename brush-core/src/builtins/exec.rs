use clap::Parser;
use std::{borrow::Cow, os::unix::process::CommandExt};

use crate::{builtins, commands, error};

/// Exec the provided command.
#[derive(Parser)]
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    #[arg(short = 'a')]
    name_for_argv0: Option<String>,

    /// Exec command with an empty environment.
    #[arg(short = 'c')]
    empty_environment: bool,

    /// Exec command as a login shell.
    #[arg(short = 'l')]
    exec_as_login: bool,

    /// Command and args.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for ExecCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, crate::error::Error> {
        if self.args.is_empty() {
            // When no arguments are present, then there's nothing for us to execute -- but we need
            // to ensure that any redirections setup for this builtin get applied to the calling
            // shell instance.
            context.shell.replace_open_files(context.params.open_files);
            return Ok(builtins::ExitCode::Success);
        }

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        let mut cmd = commands::compose_std_command(
            context.shell,
            &self.args[0],
            argv0.as_str(),
            &self.args[1..],
            context.params.open_files.clone(),
            self.empty_environment,
        )?;

        let exec_error = cmd.exec();

        if exec_error.kind() == std::io::ErrorKind::NotFound {
            Ok(builtins::ExitCode::Custom(127))
        } else {
            Err(error::Error::from(exec_error))
        }
    }
}
