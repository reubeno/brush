use clap::Parser;

use brush_core::builtins;

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

impl builtins::Command for FalseCommand {
    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, brush_core::Error> {
        Ok(builtins::ExitCode::Custom(1))
    }
}
