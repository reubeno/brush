use clap::Parser;
use std::borrow::Cow;

use brush_core::{
    ErrorKind, ExecutionResult, builtins, commands, extensions::ProcessLauncher as _,
};

/// Exec the provided command.
#[derive(Parser)]
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    #[arg(short = 'a', value_name = "NAME")]
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

brush_clap_adapter::impl_parse_args!(ExecCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ExecCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.args.is_empty() {
            // When no arguments are present, then there's nothing for us to execute -- but we need
            // to ensure that any redirections setup for this builtin get applied to the calling
            // shell instance.
            #[allow(clippy::needless_collect)]
            let fds: Vec<_> = context.iter_fds().collect();

            context.shell.replace_open_files(fds.into_iter());
            return Ok(ExecutionResult::success());
        }

        // If we know we're already running in a subshell, then `exec`ing is actually
        // unsafe, since it would also replace the *parent* shell instance. We instead
        // delegate to the `command` builtin to perform the execution, with an expectation
        // of returning.
        if context.shell.is_subshell() {
            if self.empty_environment || self.exec_as_login || self.name_for_argv0.is_some() {
                return brush_core::error::unimp("exec with options in subshell not yet supported");
            }

            let cmd_cmd = crate::command::CommandCommand {
                command_and_args: self.args.clone(),
                ..Default::default()
            };

            let shell_exits = exec_failure_ends_shell(context.shell);
            return cmd_cmd.execute(context).await.map_err(|error| {
                if shell_exits
                    && matches!(
                        error.kind(),
                        ErrorKind::CommandNotFound(..) | ErrorKind::FailedToExecuteCommand(..)
                    )
                {
                    error.into_fatal()
                } else {
                    error
                }
            });
        }

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        let shell_exits = exec_failure_ends_shell(context.shell);
        let failed = |kind: ErrorKind| {
            let error = brush_core::Error::from(kind);
            Err(if shell_exits {
                error.into_fatal()
            } else {
                error
            })
        };

        // Resolve the program as running it as a command would.
        let Some(program) = context.shell.resolve_external_command(&self.args[0]) else {
            return failed(ErrorKind::CommandNotFound(self.args[0].clone()));
        };

        let request = commands::compose_launch_request(
            &context,
            &program,
            argv0.as_str(),
            &self.args[1..],
            if self.empty_environment {
                commands::ProgramEnvironment::Empty
            } else {
                commands::ProgramEnvironment::Exported
            },
        );

        let exec_error = context.shell.process_launcher().exec(request);

        if exec_error.kind() == std::io::ErrorKind::NotFound {
            failed(ErrorKind::CommandNotFound(self.args[0].clone()))
        } else {
            failed(ErrorKind::FailedToExecuteCommand(
                self.args[0].clone(),
                exec_error,
            ))
        }
    }
}

/// Whether a failure to run the program ends the shell: it ends a
/// non-interactive shell unless `execfail` is set, and a subshell either way.
fn exec_failure_ends_shell(shell: &brush_core::Shell<impl brush_core::ShellExtensions>) -> bool {
    let options = shell.options();
    shell.is_subshell() || !(options.interactive || options.exit_on_exec_fail)
}
