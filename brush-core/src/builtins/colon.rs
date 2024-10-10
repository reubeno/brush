use clap::Parser;

use crate::{builtins, commands};

/// No-op command.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ColonCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}


impl builtins::Command for ColonCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        Ok(builtins::ExitCode::Success)
    }
}
