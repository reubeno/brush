//! The `exec` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ExecCommand);

use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, commands};
use std::{borrow::Cow, os::unix::process::CommandExt};

async fn execute<SE: brush_core::ShellExtensions>(
    command: &ExecCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.args.is_empty() {
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
        if command.empty_environment || command.exec_as_login || command.name_for_argv0.is_some() {
            return brush_core::error::unimp("exec with options in subshell not yet supported");
        }

        let cmd_cmd = crate::command::CommandCommand {
            command_and_args: command.args.clone(),
            ..Default::default()
        };

        return brush_core::builtins::Command::execute(&cmd_cmd, context).await;
    }

    let mut argv0 = Cow::Borrowed(command.name_for_argv0.as_ref().unwrap_or(&command.args[0]));

    if command.exec_as_login {
        argv0 = Cow::Owned(std::format!("-{argv0}"));
    }

    let mut cmd = commands::compose_std_command(
        &context,
        &command.args[0],
        argv0.as_str(),
        &command.args[1..],
        command.empty_environment,
    )?;

    let exec_error = cmd.exec();

    if exec_error.kind() == std::io::ErrorKind::NotFound {
        Ok(ExecutionExitCode::NotFound.into())
    } else {
        Err(ErrorKind::from(exec_error).into())
    }
}
