use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct PushdCommand {
    #[clap(short = 'n')]
    no_directory_change: bool,

    dir: String,
    //
    // TODO: implement +N and -N
    //
}

#[async_trait::async_trait]
impl BuiltinCommand for PushdCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.no_directory_change {
            log::error!("UNIMPLEMENTED: pushd -n");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let prev_working_dir = context.shell.working_dir.clone();

        let dir = std::path::Path::new(&self.dir);
        context.shell.set_working_dir(dir)?;
        context.shell.directory_stack.push(prev_working_dir);

        // Display dirs.
        let dirs_cmd = crate::builtins::dirs::DirsCommand::default();
        dirs_cmd.execute(context).await?;

        Ok(BuiltinExitCode::Success)
    }
}
