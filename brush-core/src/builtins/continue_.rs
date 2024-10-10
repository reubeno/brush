use clap::Parser;

use crate::{builtins, commands};

/// Continue to the next iteration of a control-flow loop.
#[derive(Parser)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[clap(default_value = "1")]
    which_loop: i8,
}


impl builtins::Command for ContinueCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(builtins::ExitCode::ContinueLoop(
            (self.which_loop - 1) as u8,
        ))
    }
}
