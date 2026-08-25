//! The `return_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ReturnCommand);

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult};
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ReturnCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    #[expect(clippy::cast_sign_loss)]
    let code_8bit = if let Some(code_32bit) = &command.code {
        (code_32bit & 0xFF) as u8
    } else {
        context.shell.last_exit_status()
    };

    if context.shell.in_function() || context.shell.in_sourced_script() {
        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ReturnFromFunctionOrScript;

        Ok(result)
    } else {
        let _ = writeln!(
            context.stderr(),
            "return: can only be used in a function or sourced script"
        );
        Ok(ExecutionExitCode::InvalidUsage.into())
    }
}
