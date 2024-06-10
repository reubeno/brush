use clap::Parser;

use crate::{builtin, commands};

/// Exit the shell.
#[derive(Parser)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    code: Option<i32>,
}

#[async_trait::async_trait]
impl builtin::Command for ExitCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        let code_8bit: u8;

        #[allow(clippy::cast_sign_loss)]
        if let Some(code_32bit) = &self.code {
            code_8bit = (code_32bit & 0xFF) as u8;
        } else {
            code_8bit = context.shell.last_exit_status;
        }

        Ok(builtin::ExitCode::ExitShell(code_8bit))
    }
}
