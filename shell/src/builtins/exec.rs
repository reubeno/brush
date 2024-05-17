use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

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
        if self.name_for_argv0.is_some() {
            writeln!(context.stderr(), "UNIMPLEMENTED: exec -a: name as argv[0]")?;
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.empty_environment {
            writeln!(
                context.stderr(),
                "UNIMPLEMENTED: exec -c: empty environment"
            )?;
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.exec_as_login {
            writeln!(context.stderr(), "UNIMPLEMENTED: exec -l: exec as login")?;
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if !self.args.is_empty() {
            // TODO: Do the Right Thing with the environment.
            let err = exec::Command::new(self.args[0].as_str())
                .args(&self.args[1..])
                .exec();
            match err {
                exec::Error::BadArgument(_) => Err(crate::error::Error::InvalidArguments),
                exec::Error::Errno(errno) => {
                    let io_err: std::io::Error = errno.into();

                    if io_err.kind() == std::io::ErrorKind::NotFound {
                        Ok(BuiltinExitCode::Custom(127))
                    } else {
                        Err(crate::error::Error::from(io_err))
                    }
                }
            }
        } else {
            return Ok(BuiltinExitCode::Success);
        }
    }
}
