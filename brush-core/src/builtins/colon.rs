use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// No-op command.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ColonCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for ColonCommand {
    async fn execute(
        &self,
        _context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        Ok(BuiltinExitCode::Success)
    }
}
