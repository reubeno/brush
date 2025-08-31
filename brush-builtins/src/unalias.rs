use clap::Parser;
use std::io::Write;

use brush_core::builtins;

/// Unset a shell alias.
#[derive(Parser)]
pub(crate) struct UnaliasCommand {
    /// Remove all aliases.
    #[arg(short = 'a')]
    remove_all: bool,

    /// Names of aliases to operate on.
    aliases: Vec<String>,
}

impl builtins::Command for UnaliasCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
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
