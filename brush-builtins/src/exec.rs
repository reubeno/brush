use clap::Parser;
use std::{borrow::Cow, os::unix::process::CommandExt};

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionResult, builtins, commands,
    filter::{CmdExecFilter as _, ExternalCmdParams, PreFilterResult},
    results::ExecutionWaitResult,
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

impl builtins::Command for ExecCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
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

            return cmd_cmd.execute(context).await;
        }

        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        // `exec` replaces this process image outright, so it never reaches
        // `commands::execute_external_command` and is not covered by the filtering applied
        // there. Run the same `pre_external_cmd` filter here, before `cmd.exec()`.
        //
        // The program is resolved first so the filter is shown the executable that will
        // actually run, matching what `execute_via_external` passes it; otherwise the OS
        // would redo its own `PATH` search against the child environment, which need not
        // match the shell's.
        let resolved = commands::resolve_external_program(context.shell, &self.args[0]);
        let program = resolved
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new(self.args[0].as_str()));

        let ext_cmd = commands::compose_external_command(
            context.shell,
            program,
            argv0.as_str(),
            &self.args[1..],
            self.empty_environment,
        );
        let filter_params =
            ExternalCmdParams::with_original_command(context.shell, &self.args[0], ext_cmd);
        let ext_cmd = match context
            .shell
            .cmd_exec_filter()
            .pre_external_cmd(filter_params)
            .await
        {
            PreFilterResult::Continue(params) => {
                context
                    .shell
                    .cmd_exec_filter()
                    .authorize_external_cmd(&params)
                    .await?;
                params.command
            }
            // A filter short-circuited. A denial is the `Err` case and is the point of this
            // hook; an `Ok` spawn result is unusual here but is honored rather than dropped.
            PreFilterResult::Return(output) => {
                let spawn_result = output?;
                return match spawn_result.wait().await? {
                    ExecutionWaitResult::Completed(result) => Ok(result),
                    ExecutionWaitResult::Stopped(_) => brush_core::error::unimp(
                        "filter returned a stopped process from the exec builtin",
                    ),
                };
            }
            // `PreFilterResult` is `#[non_exhaustive]`. This site is about to replace the
            // process image, so an unrecognized decision must fail closed and loudly rather
            // than be guessed at as "continue".
            _ => {
                return brush_core::error::unimp(
                    "unrecognized pre_external_cmd filter result at the exec builtin; \
                     refusing to replace the process image",
                );
            }
        };

        let mut cmd = commands::compose_filtered_std_command(&context, ext_cmd)?;

        let exec_error = cmd.exec();

        if exec_error.kind() == std::io::ErrorKind::NotFound {
            Ok(ExecutionExitCode::NotFound.into())
        } else {
            Err(ErrorKind::from(exec_error).into())
        }
    }
}
