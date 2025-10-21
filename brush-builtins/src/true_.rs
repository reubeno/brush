use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Return 0.
#[derive(Parser)]
pub(crate) struct TrueCommand {}

impl builtins::Command for TrueCommand {
    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}
