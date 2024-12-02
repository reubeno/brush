use crate::{builtins, commands};
use clap::Parser;
use std::io::Write;

/// Display the current working directory.
#[derive(Parser)]
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    #[arg(short = 'P', overrides_with = "allow_symlinks")]
    physical: bool,

    /// Print $PWD if it names the current working directory.
    #[arg(short = 'L', overrides_with = "physical")]
    allow_symlinks: bool,
}

impl builtins::Command for PwdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // POSIX: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/pwd.html

        // TODO: look for 'physical' option in execution context options (set -P)

        // if POSIXLY_CORRECT is set, we want to a logical resolution.
        // This produces a different output when doing mkdir -p a/b && ln -s a/b c && cd c && pwd
        // We should get c in this case instead of a/b at the end of the path
        let cwd = if self.physical && context.shell.env.get_str("POSIXLY_CORRECT").is_none() {
            context.shell.get_current_working_dir()
        // -L logical by default or when POSIXLY_CORRECT is set
        } else {
            context.shell.get_current_logical_working_dir()
        };

        writeln!(context.stdout(), "{}", cwd.display())?;

        Ok(builtins::ExitCode::Success)
    }
}
