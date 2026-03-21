use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Return from the current function.
#[derive(Parser)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    code: Option<i32>,
}

impl builtins::Command for ReturnCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            context.shell.last_exit_status()
        };

        if context.shell.in_function() || context.shell.in_sourced_script() {
            let mut result = ExecutionResult::new(code_8bit);
            result.next_control_flow = ExecutionControlFlow::ReturnFromFunctionOrScript;

            Ok(result)
        } else {
            let mut stderr_output = Vec::new();
            let _ = writeln!(
                stderr_output,
                "return: can only be used in a function or sourced script"
            );
            context.stderr().write_all(&stderr_output)?;
            context.stderr().flush()?;
            Ok(ExecutionExitCode::InvalidUsage.into())
        }
    }
}
