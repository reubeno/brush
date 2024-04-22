use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
pub(crate) struct ShiftCommand {
    n: Option<i32>,
}

#[async_trait::async_trait]
impl BuiltinCommand for ShiftCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let n = self.n.unwrap_or(1);

        if n < 0 {
            return Ok(BuiltinExitCode::InvalidUsage);
        }

        #[allow(clippy::cast_sign_loss)]
        let n = n as usize;

        if n > context.shell.positional_parameters.len() {
            return Ok(BuiltinExitCode::InvalidUsage);
        }

        context.shell.positional_parameters.drain(0..n);

        Ok(BuiltinExitCode::Success)
    }
}
