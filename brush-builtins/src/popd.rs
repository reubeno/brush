use brush_core::{ExecutionResult, builtins};

/// Pop a path from the current directory stack.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[arg(short = 'n')]
    no_directory_change: bool,
    // TODO(popd): implement +N and -N
}

brush_builtin_winnow::winnow_builtin!(PopdCommand);

impl builtins::Command for PopdCommand {
    type Error = crate::dirs::DirError;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(popped) = context.shell.directory_stack_mut().pop() {
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
