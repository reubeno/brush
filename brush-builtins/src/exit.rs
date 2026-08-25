use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};
use std::io::Write;

/// Exit the shell.
pub(crate) struct ExitCommand {
    code: Option<String>,
}

impl builtins::SpecCommand for ExitCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        &builtins::argmodel::CommandSpec::EMPTY
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        // N.B. Only the leading options are parsed; the remaining tokens are
        // captured verbatim via `takes_trailing_args`.
        let code = values.trailing().first().cloned();

        Ok(Self { code })
    }

    fn about() -> &'static str {
        "Exit the shell."
    }

    fn synopsis() -> &'static str {
        "[N]"
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code) = &self.code {
            if let Ok(code_32bit) = code.parse::<i64>() {
                (code_32bit & 0xFF) as u8
            } else {
                writeln!(
                    context.stderr(),
                    "{}: {}: numeric argument required",
                    context.command_name,
                    code
                )?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }
        } else {
            context.shell.last_exit_status()
        };

        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ExitShell;

        Ok(result)
    }
}
