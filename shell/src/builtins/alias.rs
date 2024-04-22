use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Manage aliases within the shell.
#[derive(Parser)]
pub(crate) struct AliasCommand {
    /// Print all defined aliases in a reusable format.
    #[arg(short = 'p')]
    print: bool,

    #[arg(name = "name[=value]")]
    aliases: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for AliasCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        //
        // TODO: implement flags
        //

        let mut exit_code = BuiltinExitCode::Success;

        if self.print || self.aliases.is_empty() {
            for (name, value) in &context.shell.aliases {
                writeln!(context.stdout(), "alias {name}='{value}'")?;
            }
        } else {
            for alias in &self.aliases {
                if let Some((name, unexpanded_value)) = alias.split_once('=') {
                    context
                        .shell
                        .aliases
                        .insert(name.to_owned(), unexpanded_value.to_owned());
                } else if let Some(value) = context.shell.aliases.get(alias) {
                    writeln!(context.stdout(), "alias {alias}='{value}'")?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {alias}: not found",
                        context.command_name
                    )?;
                    exit_code = BuiltinExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
