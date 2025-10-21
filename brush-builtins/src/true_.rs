use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Return 0.
#[derive(Parser)]
pub(crate) struct TrueCommand {}

impl builtins::Command for TrueCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
