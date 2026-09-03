use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

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
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        if self.print || self.aliases.is_empty() {
            // Aliases are stored unordered; bash lists them sorted by name.
            for (name, value) in context.shell.aliases().iter().sorted() {
                writeln!(
                    context.stdout(),
                    "alias {name}={}",
                    escape::single_quote(value)
                )?;
            }
        } else {
            for alias in &self.aliases {
                if let Some((name, unexpanded_value)) = alias.split_once('=')
                    && !name.is_empty()
                {
                    context
                        .shell
                        .aliases_mut()
                        .insert(name.to_owned(), unexpanded_value.to_owned());
                } else if let Some(value) = context.shell.aliases().get(alias) {
                    writeln!(
                        context.stdout(),
                        "alias {alias}={}",
                        escape::single_quote(value)
                    )?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {alias}: not found",
                        context.command_name
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }

        Ok(exit_code)
    }
}
