use clap::Parser;
use std::io::Write;

use crate::{
    builtins,
    commands,
    alias_events::{self, AliasEvent},
};

/// Manage aliases within the shell.
#[derive(Parser)]
pub(crate) struct AliasCommand {
    /// Print all defined aliases in a reusable format.
    #[arg(short = 'p')]
    print: bool,

    /// List of aliases to display or update.
    #[arg(name = "name[=value]")]
    aliases: Vec<String>,
}

impl builtins::Command for AliasCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut exit_code = builtins::ExitCode::Success;

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
                    alias_events::emit(AliasEvent::Set {
                        name: name.to_owned(),
                        value: unexpanded_value.to_owned(),
                    });
                } else if let Some(value) = context.shell.aliases.get(alias) {
                    writeln!(context.stdout(), "alias {alias}='{value}'")?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {alias}: not found",
                        context.command_name
                    )?;
                    exit_code = builtins::ExitCode::Custom(1);
                }
            }
        }

        Ok(exit_code)
    }
}
