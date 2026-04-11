use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins};

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
        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        if self.print || self.aliases.is_empty() {
            for (name, value) in context.shell.aliases() {
                writeln!(output, "alias {name}='{value}'")?;
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
                    writeln!(output, "alias {alias}='{value}'")?;
                } else {
                    writeln!(
                        stderr_output,
                        "{}: {alias}: not found",
                        context.command_name
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }

        // Write output async
        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        if !stderr_output.is_empty() {
            context.stderr().write_all(&stderr_output)?;
            context.stderr().flush()?;
        }

        Ok(exit_code)
    }
}
