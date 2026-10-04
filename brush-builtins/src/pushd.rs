use brush_core::{ExecutionResult, builtins};

/// Push a path onto the current directory stack.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[arg(short = 'n')]
    no_directory_change: bool,

    /// Directory to push on the directory stack.
    #[arg(positional)]
    dir: String,
    // TODO(pushd): implement +N and -N
}

brush_builtin_winnow::winnow_builtin!(PushdCommand);

impl builtins::Command for PushdCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.no_directory_change {
            context
                .shell
                .directory_stack_mut()
                .push(std::path::PathBuf::from(&self.dir));
        } else {
            let prev_working_dir = context.shell.working_dir().to_path_buf();

            let dir = std::path::Path::new(&self.dir);
            context.shell.set_working_dir(dir)?;

            context.shell.directory_stack_mut().push(prev_working_dir);
        }

        // Display dirs.
        let dirs_cmd = crate::dirs::DirsCommand::default();
        dirs_cmd.execute(context).await?;

        Ok(ExecutionResult::success())
    }
}
