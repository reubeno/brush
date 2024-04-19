use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
pub(crate) struct BreakCommand {
    #[clap(default_value = "1")]
    which_loop: i8,
}

#[async_trait::async_trait]
impl BuiltinCommand for BreakCommand {
    async fn execute(
        &self,
        _context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(BuiltinExitCode::InvalidUsage);
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(BuiltinExitCode::BreakLoop((self.which_loop - 1) as u8))
    }
}
