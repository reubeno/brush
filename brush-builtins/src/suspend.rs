//! The `suspend` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(SuspendCommand);

use brush_core::{ExecutionExitCode, ExecutionResult};
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &SuspendCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if context.shell.options().login_shell && !command.force {
        writeln!(context.stderr(), "login shell cannot be suspended")?;
        return Ok(ExecutionExitCode::InvalidUsage.into());
    }

    #[expect(clippy::cast_possible_wrap)]
    brush_core::sys::signal::kill_process(
        std::process::id() as i32,
        brush_core::traps::TrapSignal::Signal(nix::sys::signal::SIGSTOP),
    )?;

    Ok(ExecutionResult::success())
}
