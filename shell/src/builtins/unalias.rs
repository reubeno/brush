use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Unset a shell alias.
#[derive(Parser)]
pub(crate) struct UnaliasCommand {
    #[arg(short = 'a')]
    remove_all: bool,

    aliases: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for UnaliasCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut exit_code = BuiltinExitCode::Success;

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
                    exit_code = BuiltinExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
