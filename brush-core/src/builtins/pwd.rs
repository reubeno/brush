use crate::{builtin, commands};
use clap::Parser;
use std::io::Write;

/// Display the current working directory.
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
impl builtin::Command for PwdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        //
        // TODO: implement flags
        // TODO: look for 'physical' option in execution context
        //

        if self.physical || self.allow_symlinks {
            writeln!(context.stderr(), "UNIMPLEMENTED: pwd with -P or -L")?;
            return Ok(builtin::ExitCode::Unimplemented);
        }

        let cwd = context.shell.working_dir.to_string_lossy().into_owned();

        writeln!(context.stdout(), "{cwd}")?;

        Ok(builtin::ExitCode::Success)
    }
}
