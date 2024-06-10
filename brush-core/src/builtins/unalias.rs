use clap::Parser;
use std::io::Write;

use crate::{builtin, commands};

/// Unset a shell alias.
#[derive(Parser)]
pub(crate) struct UnaliasCommand {
    /// Remove all aliases.
    #[arg(short = 'a')]
    remove_all: bool,

    /// Names of aliases to operate on.
    aliases: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for UnaliasCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        let mut exit_code = builtin::ExitCode::Success;

        if self.remove_all {
            context.shell.aliases.clear();
        } else {
            for alias in &self.aliases {
                if context.shell.aliases.remove(alias).is_none() {
                    writeln!(
                        context.stderr(),
                        "{}: {}: not found",
                        context.command_name,
                        alias
                    )?;
                    exit_code = builtin::ExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
