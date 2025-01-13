use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error};

/// Suspend the shell.
#[derive(Parser)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[arg(short = 'f')]
    force: bool,
}

impl builtins::Command for SuspendCommand {
    async fn execute(
        self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        if context.shell.options.login_shell && !self.force {
            writeln!(context.stderr(), "login shell cannot be suspended")?;
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_wrap)]
        crate::sys::signal::kill_process(
            std::process::id() as i32,
            crate::traps::TrapSignal::Signal(nix::sys::signal::SIGSTOP),
        )?;

        Ok(builtins::ExitCode::Success)
    }
}
