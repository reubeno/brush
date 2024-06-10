use clap::Parser;

use crate::{builtin, commands};

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

#[async_trait::async_trait]
impl builtin::Command for FalseCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<builtin::ExitCode, crate::error::Error> {
        Ok(builtin::ExitCode::Custom(1))
    }
}
