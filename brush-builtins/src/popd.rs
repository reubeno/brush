//! The `popd` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(PopdCommand);

use brush_core::ExecutionResult;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &PopdCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, crate::dirs::DirError> {
    if let Some(popped) = context.shell.directory_stack_mut().pop() {
        if !command.no_directory_change {
            context.shell.set_working_dir(&popped)?;
        }

        // Display dirs.
        let dirs_cmd = crate::dirs::DirsCommand::default();
        brush_core::builtins::Command::execute(&dirs_cmd, context).await?;

        Ok(ExecutionResult::success())
    } else {
        Err(crate::dirs::DirError::DirStackEmpty)
    }
}
