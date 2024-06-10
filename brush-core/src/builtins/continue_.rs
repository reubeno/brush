use clap::Parser;

use crate::{builtin, commands};

/// Continue to the next iteration of a control-flow loop.
#[derive(Parser)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[clap(default_value = "1")]
    which_loop: i8,
}

#[async_trait::async_trait]
impl builtin::Command for ContinueCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(builtin::ExitCode::InvalidUsage);
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(builtin::ExitCode::ContinueLoop((self.which_loop - 1) as u8))
    }
}
