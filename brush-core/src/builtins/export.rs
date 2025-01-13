use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::{
    builtins, commands,
    env::{EnvironmentLookup, EnvironmentScope},
    variables,
};

/// Add or update exported shell variables.
#[derive(Parser)]
pub(crate) struct ExportCommand {
    /// Names are treated as function names.
    #[arg(short = 'f')]
    names_are_functions: bool,

    /// Un-export the names.
    #[arg(short = 'n')]
    unexport: bool,

    /// Display all exported names.
    #[arg(short = 'p')]
    display_exported_names: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[clap(skip)]
    declarations: Vec<commands::CommandArg>,
}

impl builtins::DeclarationCommand for ExportCommand {
    fn set_declarations(&mut self, declarations: Vec<commands::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for ExportCommand {
    async fn execute(
        self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if !self.declarations.is_empty() {
            for decl in &self.declarations {
                match decl {
                    commands::CommandArg::String(s) => {
                        // Try to find the variable already present; if we find it, then mark it
                        // exported.
                        if let Some((_, variable)) = context.shell.env.get_mut(s) {
                            variable.export();
                        }
                    }
                    commands::CommandArg::Assignment(assignment) => {
                        let name = match &assignment.name {
                            brush_parser::ast::AssignmentName::VariableName(name) => name,
                            brush_parser::ast::AssignmentName::ArrayElementName(_, _) => {
                                writeln!(context.stderr(), "not a valid variable name")?;
                                return Ok(builtins::ExitCode::InvalidUsage);
                            }
                        };

                        let value = match &assignment.value {
                            brush_parser::ast::AssignmentValue::Scalar(s) => {
                                variables::ShellValueLiteral::Scalar(s.flatten())
                            }
                            brush_parser::ast::AssignmentValue::Array(a) => {
                                variables::ShellValueLiteral::Array(variables::ArrayLiteral(
                                    a.iter()
                                        .map(|(k, v)| {
                                            (k.as_ref().map(|k| k.flatten()), v.flatten())
                                        })
                                        .collect(),
                                ))
                            }
                        };

                        // Update the variable with the provided value and then mark it exported.
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

        Ok(builtins::ExitCode::Success)
    }
}
