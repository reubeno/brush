use clap::Parser;

use crate::{builtin, commands};

/// Return 0.
#[derive(Parser)]
pub(crate) struct TrueCommand {}

#[async_trait::async_trait]
impl builtin::Command for TrueCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        Ok(builtin::ExitCode::Success)
    }
}
