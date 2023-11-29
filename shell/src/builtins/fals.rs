use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct FalseCommand {}

impl BuiltinCommand for FalseCommand {
    fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        Ok(BuiltinExitCode::Custom(1))
    }
}
