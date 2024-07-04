use crate::{builtins, commands};
use clap::Parser;
use std::io::Write;

/// Display the current working directory.
#[derive(Parser)]
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    #[arg(short = 'P')]
    physical: bool,

    /// Print $PWD if it names the current working directory.
    #[arg(short = 'L')]
    allow_symlinks: bool,
}

#[async_trait::async_trait]
impl builtins::Command for PwdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        //
        // TODO: implement flags
        // TODO: look for 'physical' option in execution context
        //

        if self.physical || self.allow_symlinks {
            writeln!(context.stderr(), "UNIMPLEMENTED: pwd with -P or -L")?;
            return Ok(builtins::ExitCode::Unimplemented);
        }

        let cwd = context.shell.working_dir.to_string_lossy().into_owned();

        writeln!(context.stdout(), "{cwd}")?;

        Ok(builtins::ExitCode::Success)
    }
}
