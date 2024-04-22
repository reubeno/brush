use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
pub(crate) struct TrueCommand {}

#[async_trait::async_trait]
impl BuiltinCommand for TrueCommand {
    async fn execute(
        &self,
        _context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        Ok(BuiltinExitCode::Success)
    }
}
