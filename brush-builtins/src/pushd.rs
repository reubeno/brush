use brush_core::{ExecutionResult, builtins};

/// Push a path onto the current directory stack.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[arg(short = 'n')]
    no_directory_change: bool,

    /// Directory to push on the directory stack. Without one, the top two
    /// directories on the stack are exchanged.
    #[arg(positional)]
    dir: Option<String>,
    // TODO(pushd): implement +N and -N
}

brush_winnow_args_adapter::impl_parse_args!(PushdCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for PushdCommand {
    type Error = crate::dirs::DirError;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(dir) = &self.dir {
            if self.no_directory_change {
                context
                    .shell
                    .directory_stack_mut()
                    .push(std::path::PathBuf::from(dir));
            } else {
                let prev_working_dir = context.shell.working_dir().to_path_buf();

                let dir = std::path::Path::new(dir);
                context.shell.set_working_dir(dir)?;

                context.shell.directory_stack_mut().push(prev_working_dir);
            }
        } else {
            // With no directory, exchange the current directory with the one on top of
            // the stack.
            let Some(top) = context.shell.directory_stack().last().cloned() else {
                return Err(crate::dirs::DirError::NoOtherDirectory);
            };

            // As in bash, -n leaves both the stack and the current directory alone here.
            if self.no_directory_change {
                return Ok(ExecutionResult::success());
            }

            let prev_working_dir = context.shell.working_dir().to_path_buf();
            context.shell.set_working_dir(&top)?;

            let stack = context.shell.directory_stack_mut();
            stack.pop();
            stack.push(prev_working_dir);
        }

        // Display dirs.
        let dirs_cmd = crate::dirs::DirsCommand::default();
        // The change to the stack decides the status, not the listing.
        let _ = dirs_cmd.execute(context).await?;

        Ok(ExecutionResult::success())
    }
}
