use std::io::Write;
use usage::Cli;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Suspend the shell.
#[derive(Cli)]
#[usage(bin = "suspend", unknown_flags = "error", args_override_self = false)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[usage(short = 'f')]
    force: bool,
}

brush_builtin_usage::usage_builtin!(SuspendCommand);

impl builtins::Command for SuspendCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if context.shell.options().login_shell && !self.force {
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
}
