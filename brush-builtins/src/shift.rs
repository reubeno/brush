use clap::Parser;

use brush_core::builtins;

/// Shift positional arguments.
#[derive(Parser)]
pub(crate) struct ShiftCommand {
    /// Number of positions to shift the arguments by (defaults to 1).
    n: Option<i32>,
}

impl builtins::Command for ShiftCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        let n = self.n.unwrap_or(1);

        if n < 0 {
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        #[expect(clippy::cast_sign_loss)]
        let n = n as usize;

        if n > context.shell.positional_parameters.len() {
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        context.shell.positional_parameters.drain(0..n);

        Ok(builtins::ExitCode::Success)
    }
}
