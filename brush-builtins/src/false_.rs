use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

impl builtins::Command for FalseCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::general_error())
    }
}
