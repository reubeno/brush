use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    env::{EnvironmentLookup, EnvironmentScope},
};

#[derive(Parser, Debug)]
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
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if !self.names.is_empty() {
            for name in &self.names {
                // See if we have a name=value pair; if so, then update the variable
                // with the provided value and then mark it exported.
                if let Some((name, value)) = name.split_once('=') {
                    context.shell.env.update_or_add(
                        name,
                        value,
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
                    if let Some(variable) = context.shell.env.get_mut(name) {
                        variable.exported = true;
                    }
                }
            }
        } else {
            // Enumerate variables, sorted by key.
            for (name, variable) in context.shell.env.iter().sorted_by_key(|v| v.0) {
                if variable.exported {
                    println!("declare -x {}=\"{}\"", name, String::from(&variable.value));
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
