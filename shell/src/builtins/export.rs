use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

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

impl BuiltinCommand for ExportCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if self.names.len() > 0 {
            for name in &self.names {
                // See if we have a name=value pair; if so, then update the variable
                // with the provided value and then mark it exported.
                if let Some((name, value)) = name.split_once('=') {
                    context.shell.set_var(name, value, true, false)?;
                } else {
                    // Try to find the variable already present; if we find it, then mark it
                    // exported.
                    if let Some(variable) = context.shell.variables.get_mut(name) {
                        variable.exported = true;
                    }
                }
            }
        } else {
            for (name, variable) in &context.shell.variables {
                if variable.exported {
                    println!("declare -x {}=\"{}\"", name, variable.value.as_str());
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
