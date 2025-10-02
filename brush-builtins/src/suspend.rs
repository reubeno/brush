use clap::Parser;
use std::io::Write;

use brush_core::builtins;

/// Suspend the shell.
#[derive(Parser)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[arg(short = 'f')]
    force: bool,
}

impl builtins::Command for SuspendCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, brush_core::Error> {
        if context.shell.options.login_shell && !self.force {
            writeln!(context.stderr(), "login shell cannot be suspended")?;
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        #[expect(clippy::cast_possible_wrap)]
        brush_core::sys::signal::kill_process(
            std::process::id() as i32,
            brush_core::traps::TrapSignal::Signal(nix::sys::signal::SIGSTOP),
        )?;

        Ok(builtins::ExitCode::Success)
    }
}
