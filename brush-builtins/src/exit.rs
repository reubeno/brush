//! The `exit` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ExitCommand);

use brush_core::{ExecutionControlFlow, ExecutionResult};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ExitCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    #[expect(clippy::cast_sign_loss)]
    let code_8bit = if let Some(code_32bit) = &command.code {
        (code_32bit & 0xFF) as u8
    } else {
        context.shell.last_exit_status()
    };

    let mut result = ExecutionResult::new(code_8bit);
    result.next_control_flow = ExecutionControlFlow::ExitShell;

    Ok(result)
}
