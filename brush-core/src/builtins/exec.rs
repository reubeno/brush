use clap::Parser;
use std::{borrow::Cow, os::unix::process::CommandExt};

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    commands, error,
};

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

#[async_trait::async_trait]
impl BuiltinCommand for ExecCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.args.is_empty() {
            return Ok(BuiltinExitCode::Success);
        }

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        let (mut cmd, here_doc) = commands::compose_std_command(
            context.shell,
            &self.args[0],
            argv0.as_str(),
            &self.args[1..],
            context.open_files.clone(),
            self.empty_environment,
        )?;

        if here_doc.is_some() {
            return error::unimp("exec with here doc");
        }

        let exec_error = cmd.exec();

        if exec_error.kind() == std::io::ErrorKind::NotFound {
            Ok(BuiltinExitCode::Custom(127))
        } else {
            Err(crate::error::Error::from(exec_error))
        }
    }
}
