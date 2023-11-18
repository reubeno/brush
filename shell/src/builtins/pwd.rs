use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
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

impl BuiltinCommand for PwdCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        //
        // TODO: implement flags
        // TODO: look for 'physical' option in execution context
        //

        if self.physical || self.allow_symlinks {
            log::error!("UNIMPLEMENTED: pwd with -P or -L");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let cwd = context.context.working_dir.to_string_lossy().into_owned();

        // TODO: Need to print to whatever the stdout is for the shell.
        println!("{}", cwd);

        Ok(BuiltinExitCode::Success)
    }
}
