use clap::Parser;

use crate::{builtins, commands};

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

#[async_trait::async_trait]
impl builtins::Command for FalseCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, crate::error::Error> {
        Ok(builtins::ExitCode::Custom(1))
    }
}
