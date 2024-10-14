use clap::Parser;
use std::io::Write;

use crate::{builtins, commands};

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
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if let Some(popped) = context.shell.directory_stack.pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::builtins::dirs::DirsCommand::default();
            dirs_cmd.execute(context).await?;
        } else {
            writeln!(context.stderr(), "popd: directory stack empty")?;
            return Ok(builtins::ExitCode::Custom(1));
        }

        Ok(builtins::ExitCode::Success)
    }
}
