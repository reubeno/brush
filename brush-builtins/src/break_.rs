use clap::Parser;

use brush_core::builtins;

/// Breaks out of a control-flow loop.
#[derive(Parser)]
pub(crate) struct BreakCommand {
    /// If specified, indicates which nested loop to break out of.
    #[clap(default_value_t = 1)]
    which_loop: i8,
}

impl builtins::Command for BreakCommand {
    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        #[expect(clippy::cast_sign_loss)]
        Ok(builtins::ExitCode::BreakLoop((self.which_loop - 1) as u8))
    }
}
