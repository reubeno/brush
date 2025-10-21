use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

impl builtins::Command for FalseCommand {
    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::new(1))
    }
}
