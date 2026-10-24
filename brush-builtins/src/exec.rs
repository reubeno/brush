use clap::Parser;
use std::borrow::Cow;
use std::io::Write as _;

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionResult, builtins, commands,
    extensions::ProcessLauncher as _,
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
            let program = &self.args[0];
            let found = context
                .shell
                .resolve_external_command(program)
                .is_some_and(|path| context.shell.absolute_path(path).exists());

            // Real `exec` replaces the process, so the subshell's own EXIT trap never runs;
            // this delegation returns instead, so drop that trap before it does. bash keeps it
            // only for a name it couldn't find in PATH, where it exits before resetting traps.
            if found || brush_core::sys::fs::contains_path_separator(program) {
                context
                    .shell
                    .traps_mut()
                    .remove_handlers(brush_core::traps::TrapSignal::Exit);
            }

            if self.empty_environment || self.exec_as_login || self.name_for_argv0.is_some() {
                return brush_core::error::unimp("exec with options in subshell not yet supported");
            }

            // Real `exec` of a program that can't be found ends the shell with 127.
            if !found {
                let _ = writeln!(context.stderr(), "exec: {program}: not found");
                let mut result = ExecutionResult::from(ExecutionExitCode::NotFound);
                result.next_control_flow = brush_core::ExecutionControlFlow::ExitShell;
                return Ok(result);
            }

            let cmd_cmd = crate::command::CommandCommand {
                command_and_args: self.args.clone(),
                ..Default::default()
            };

            // Nothing that follows `exec` runs in the real shell, so end the subshell here.
            let mut result = cmd_cmd.execute(context).await?;
            result.next_control_flow = brush_core::ExecutionControlFlow::ExitShell;
            return Ok(result);
        }

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        // Resolve the program as running it as a command would.
        let Some(program) = context.shell.resolve_external_command(&self.args[0]) else {
            return Ok(ExecutionExitCode::NotFound.into());
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
            Ok(ExecutionExitCode::NotFound.into())
        } else {
            Err(ErrorKind::from(exec_error).into())
        }
    }
}
