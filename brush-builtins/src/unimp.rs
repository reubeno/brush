//! The `unimp` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(UnimplementedCommand);

use brush_core::{ExecutionExitCode, trace_categories};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &UnimplementedCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    tracing::warn!(target: trace_categories::UNIMPLEMENTED,
        "unimplemented built-in: {} {}",
        context.command_name,
        command.args.join(" ")
    );
    Ok(ExecutionExitCode::Unimplemented.into())
}
