use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct ColonCommand {}

#[async_trait::async_trait]
impl BuiltinCommand for ColonCommand {
    async fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        Ok(BuiltinExitCode::Success)
    }
}
