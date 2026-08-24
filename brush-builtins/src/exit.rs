use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Exit the shell.
pub(crate) struct ExitCommand {
    /// The exit code to return.
    code: Option<String>,
}

impl builtins::Command for ExitCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let code = bpaf::pure(None);

        bpaf::construct!(ExitCommand { code })
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

    fn set_trailing_args(&mut self, mut args: Vec<String>) {
        self.code = if args.is_empty() {
            None
        } else {
            Some(args.remove(0))
        };
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
