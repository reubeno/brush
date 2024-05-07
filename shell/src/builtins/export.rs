use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    env::{EnvironmentLookup, EnvironmentScope},
    variables,
};

#[derive(Parser)]
pub(crate) struct ExportCommand {
    #[arg(short = 'f')]
    names_are_functions: bool,

    #[arg(short = 'n')]
    unexport: bool,

    #[arg(short = 'p')]
    display_exported_names: bool,

    #[arg(name = "name[=value]")]
    names: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for ExportCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if !self.names.is_empty() {
            for name in &self.names {
                // See if we have a name=value pair; if so, then update the variable
                // with the provided value and then mark it exported.
                if let Some((name, value)) = name.split_once('=') {
                    context.shell.env.update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(value.to_owned()),
                        |var| {
                            var.export();
                            Ok(())
                        },
                        EnvironmentLookup::Anywhere,
                        EnvironmentScope::Global,
                    )?;
                } else {
                    // Try to find the variable already present; if we find it, then mark it
                    // exported.
                    if let Some((_, variable)) = context.shell.env.get_mut(name) {
                        variable.export();
                    }
                }
            }
        } else {
            // Enumerate variables, sorted by key.
            for (name, variable) in context.shell.env.iter().sorted_by_key(|v| v.0) {
                if variable.is_exported() {
                    writeln!(
                        context.stdout(),
                        "declare -x {}=\"{}\"",
                        name,
                        variable.value().to_cow_string()
                    )?;
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
