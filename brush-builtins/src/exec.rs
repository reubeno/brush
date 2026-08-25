use std::{borrow::Cow, os::unix::process::CommandExt};

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionResult,
    argmodel::{ArgSpec, CommandSpec},
    builtins, commands,
};

/// Exec the provided command.
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    name_for_argv0: Option<String>,

    /// Exec command with an empty environment.
    empty_environment: bool,

    /// Exec command as a login shell.
    exec_as_login: bool,

    /// Command and args.
    args: Vec<String>,
}

const ID_NAME_FOR_ARGV0: &str = "name_for_argv0";
const ID_EMPTY_ENVIRONMENT: &str = "empty_environment";
const ID_EXEC_AS_LOGIN: &str = "exec_as_login";

impl builtins::SpecCommand for ExecCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            args: &[
                ArgSpec::value(
                    ID_NAME_FOR_ARGV0,
                    &['a'],
                    &[],
                    "NAME",
                    "Pass given name as zeroth argument to command.",
                ),
                ArgSpec::flag(
                    ID_EMPTY_ENVIRONMENT,
                    &['c'],
                    &[],
                    "Exec command with an empty environment.",
                ),
                ArgSpec::flag(
                    ID_EXEC_AS_LOGIN,
                    &['l'],
                    &[],
                    "Exec command as a login shell.",
                ),
            ],
            positionals: &[],
        };
        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            name_for_argv0: values.value(ID_NAME_FOR_ARGV0).map(str::to_string),
            empty_environment: values.flag(ID_EMPTY_ENVIRONMENT),
            exec_as_login: values.flag(ID_EXEC_AS_LOGIN),
            args: values.trailing().to_vec(),
        })
    }

    fn about() -> &'static str {
        "Exec the provided command."
    }

    fn synopsis() -> &'static str {
        "[-acl] [COMMAND [ARG]...]"
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn value_taking_short_options() -> &'static str {
        "a"
    }

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

        let mut cmd = commands::compose_std_command(
            &context,
            &self.args[0],
            argv0.as_str(),
            &self.args[1..],
            self.empty_environment,
        )?;

        let exec_error = cmd.exec();

        if exec_error.kind() == std::io::ErrorKind::NotFound {
            Ok(ExecutionExitCode::NotFound.into())
        } else {
            Err(ErrorKind::from(exec_error).into())
        }
    }
}
