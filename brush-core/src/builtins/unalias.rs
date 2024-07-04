use clap::Parser;
use std::io::Write;

use crate::{builtins, commands};

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
impl builtins::Command for UnaliasCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut exit_code = builtins::ExitCode::Success;

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
                    exit_code = builtins::ExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
