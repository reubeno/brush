use clap::Parser;
use std::borrow::Cow;

use brush_core::{ErrorKind, ExecutionResult, builtins};

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

impl builtins::Command for ExecCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
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

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        // Resolve the program as running it as a command would.
        let Some(program) = context.shell.resolve_external_command(&self.args[0]) else {
            return Err(ErrorKind::CommandNotFound(self.args[0].clone()).into());
        };

        let mut request = context.compose_launch_request(&program, argv0.as_str(), &self.args[1..]);
        if self.empty_environment {
            request.env.clear();
        }

        // A failure to start the program is reported by its name, not `exec`.
        self.args[0].clone_into(&mut context.command_name);

        // A subshell can't replace its process, which would end the shell running it
        // too, so it starts the program and waits for it instead.
        // TODO(exec): end the subshell once the program exits.
        if context.shell.is_subshell() {
            let wait_result = context.spawn_program(request)?.wait().await?;
            return Ok(wait_result.into());
        }

        Err(context.exec_program(request))
    }
}
