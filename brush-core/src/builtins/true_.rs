use clap::Parser;

use crate::{builtins, commands};

/// Return 0.
#[derive(Parser)]
pub(crate) struct TrueCommand {}

impl builtins::Command for TrueCommand {
    async fn execute(
        self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        Ok(builtins::ExitCode::Success)
    }
}
