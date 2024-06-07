use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Return a non-zero exit code.
#[derive(Parser)]
pub(crate) struct FalseCommand {}

#[async_trait::async_trait]
impl BuiltinCommand for FalseCommand {
    async fn execute(
        &self,
        _context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        Ok(BuiltinExitCode::Custom(1))
    }
}
