//! The `continue_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ContinueCommand);

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ContinueCommand,
    _context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // If specified, which_loop needs to be positive.
    if command.which_loop <= 0 {
        return Ok(ExecutionExitCode::InvalidUsage.into());
    }

    let mut result = ExecutionResult::success();

    result.next_control_flow = ExecutionControlFlow::ContinueLoop {
        #[expect(clippy::cast_sign_loss)]
        levels: (command.which_loop - 1) as usize,
    };

    Ok(result)
}
