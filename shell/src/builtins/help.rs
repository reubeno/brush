use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub(crate) struct HelpCommand {}

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl BuiltinCommand for HelpCommand {
    fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        println!("brush version {VERSION}");
        Ok(BuiltinExitCode::Success)
    }
}
