//! The `shift` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ShiftCommand);

use brush_core::{ExecutionExitCode, ExecutionResult};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ShiftCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let n = command.n.unwrap_or(1);

    if n < 0 {
        return Ok(ExecutionExitCode::InvalidUsage.into());
    }

    #[expect(clippy::cast_sign_loss)]
    let n = n as usize;

    let args = context.shell.current_shell_args_mut();

    if n > args.len() {
        return Ok(ExecutionExitCode::InvalidUsage.into());
    }

    args.drain(0..n);

    Ok(ExecutionResult::success())
}
