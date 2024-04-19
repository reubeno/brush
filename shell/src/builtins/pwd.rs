use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;
use std::io::Write;

#[derive(Parser)]
pub(crate) struct PwdCommand {
    #[arg(
        short = 'P',
        help = "print the physical directory, without any symbolic links"
    )]
    physical: bool,
    #[arg(
        short = 'L',
        help = "print the value of $PWD if it names the current working directory"
    )]
    allow_symlinks: bool,
}

#[async_trait::async_trait]
impl BuiltinCommand for PwdCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        //
        // TODO: implement flags
        // TODO: look for 'physical' option in execution context
        //

        if self.physical || self.allow_symlinks {
            writeln!(context.stderr(), "UNIMPLEMENTED: pwd with -P or -L")?;
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let cwd = context.shell.working_dir.to_string_lossy().into_owned();

        writeln!(context.stdout(), "{cwd}")?;

        Ok(BuiltinExitCode::Success)
    }
}
