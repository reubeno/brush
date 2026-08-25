//! The `pushd` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(PushdCommand);

use brush_core::ExecutionResult;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &PushdCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.no_directory_change {
        context
            .shell
            .directory_stack_mut()
            .push(std::path::PathBuf::from(&command.dir));
    } else {
        let prev_working_dir = context.shell.working_dir().to_path_buf();

        let dir = std::path::Path::new(&command.dir);
        context.shell.set_working_dir(dir)?;

        context.shell.directory_stack_mut().push(prev_working_dir);
    }

    // Display dirs.
    let dirs_cmd = crate::dirs::DirsCommand::default();
    brush_core::builtins::Command::execute(&dirs_cmd, context).await?;

    Ok(ExecutionResult::success())
}
