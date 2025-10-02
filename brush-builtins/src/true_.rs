use clap::Parser;

use brush_core::builtins;

/// Return 0.
#[derive(Parser)]
pub(crate) struct TrueCommand {}

impl builtins::Command for TrueCommand {
    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        Ok(builtins::ExitCode::Success)
    }
}
