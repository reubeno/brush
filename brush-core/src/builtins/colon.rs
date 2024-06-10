use clap::Parser;

use crate::{builtin, commands};

/// No-op command.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ColonCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for ColonCommand {
    async fn execute(
        &self,
        _context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        Ok(builtin::ExitCode::Success)
    }
}
