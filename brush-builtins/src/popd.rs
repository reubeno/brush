use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Pop a path from the current directory stack.
#[derive(Parser)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,
    //
    // TODO: implement +N and -N
}

impl builtins::Command for PopdCommand {
    type Error = crate::dirs::DirError;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(popped) = context.shell.directory_stack.pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::dirs::DirsCommand::default();
            dirs_cmd.execute(context).await?;

            Ok(ExecutionResult::success())
        } else {
            Err(crate::dirs::DirError::DirStackEmpty)
        }
    }
}
