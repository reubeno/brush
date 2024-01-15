use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct ShiftCommand {
    n: Option<i32>,
}

#[async_trait::async_trait]
impl BuiltinCommand for ShiftCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
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
