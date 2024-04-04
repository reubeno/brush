use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;
use std::io::Write;

#[derive(Parser)]
pub(crate) struct HelpCommand {}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait::async_trait]
impl BuiltinCommand for HelpCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        writeln!(context.stdout(), "brush version {VERSION}")?;
        Ok(BuiltinExitCode::Success)
    }
}
