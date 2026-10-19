use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Pop a path from the current directory stack.
#[derive(Parser)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,
    //
    // TODO(popd): implement +N and -N
}

brush_clap_adapter::impl_parse_args!(PopdCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for PopdCommand {
    type Error = crate::dirs::DirError;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(popped) = context.shell.directory_stack_mut().pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::dirs::DirsCommand::default();
            // The change to the stack decides the status, not the listing.
            let _ = dirs_cmd.execute(context).await?;

            Ok(ExecutionResult::success())
        } else {
            Err(crate::dirs::DirError::DirStackEmpty)
        }
    }
}
