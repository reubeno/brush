use clap::Parser;

use crate::{builtins, commands};

/// Push a path onto the current directory stack.
#[derive(Parser)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,

    /// Directory to push on the directory stack.
    dir: String,
    //
    // TODO: implement +N and -N
}

impl builtins::Command for PushdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.no_directory_change {
            context
                .shell
                .directory_stack
                .push(std::path::PathBuf::from(&self.dir));
        } else {
            let prev_working_dir = context.shell.working_dir.clone();

            let dir = std::path::Path::new(&self.dir);
            context.shell.set_working_dir(dir)?;

            context.shell.directory_stack.push(prev_working_dir);
        }

        // Display dirs.
        let dirs_cmd = crate::builtins::dirs::DirsCommand::default();
        dirs_cmd.execute(context).await?;

        Ok(builtins::ExitCode::Success)
    }
}
