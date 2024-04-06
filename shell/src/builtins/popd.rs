use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
pub(crate) struct PopdCommand {
    #[clap(short = 'n')]
    no_directory_change: bool,
    //
    // TODO: implement +N and -N
    //
}

#[async_trait::async_trait]
impl BuiltinCommand for PopdCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if let Some(popped) = context.shell.directory_stack.pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::builtins::dirs::DirsCommand::default();
            dirs_cmd.execute(context).await?;
        } else {
            log::error!("popd: directory stack empty");
            return Ok(BuiltinExitCode::Custom(1));
        }

        Ok(BuiltinExitCode::Success)
    }
}
