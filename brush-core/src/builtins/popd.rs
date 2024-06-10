use clap::Parser;
use std::io::Write;

use crate::{builtin, commands};

/// Pop a path from the current directory stack.
#[derive(Parser)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,
    //
    // TODO: implement +N and -N
    //
}

#[async_trait::async_trait]
impl builtin::Command for PopdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        if let Some(popped) = context.shell.directory_stack.pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::builtins::dirs::DirsCommand::default();
            dirs_cmd.execute(context).await?;
        } else {
            writeln!(context.stderr(), "popd: directory stack empty")?;
            return Ok(builtin::ExitCode::Custom(1));
        }

        Ok(builtin::ExitCode::Success)
    }
}
