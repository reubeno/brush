use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::{
    builtins, commands,
    env::{EnvironmentLookup, EnvironmentScope},
    error, variables,
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
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.declarations.is_empty() {
            display_all_exported_vars(&context)?;
            return Ok(builtins::ExitCode::Success);
        }

        let mut exit_code = builtins::ExitCode::Success;
        for decl in &self.declarations {
            let current_exit_code = self.process_decl(&mut context, decl)?;
            if !matches!(current_exit_code, builtins::ExitCode::Success) {
                exit_code = current_exit_code;
            }
        }

        Ok(exit_code)
    }
}

impl ExportCommand {
    fn process_decl(
        &self,
        context: &mut commands::ExecutionContext<'_>,
        decl: &commands::CommandArg,
    ) -> Result<builtins::ExitCode, error::Error> {
        match decl {
            commands::CommandArg::String(s) => {
                // See if this is supposed to be a function name.
                if self.names_are_functions {
                    // Try to find the function already present; if we find it, then mark it
                    // exported.
                    if let Some(func) = context.shell.funcs.get_mut(s) {
                        if self.unexport {
                            func.unexport();
                        } else {
                            func.export();
                        }
                    } else {
                        writeln!(context.stderr(), "{s}: not a function")?;
                        return Ok(builtins::ExitCode::InvalidUsage);
                    }
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported.
                else if let Some((_, variable)) = context.shell.env.get_mut(s) {
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
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
                                .map(|(k, v)| (k.as_ref().map(|k| k.flatten()), v.flatten()))
                                .collect(),
                        ))
                    }
                };

                // Update the variable with the provided value and then mark it exported.
                context.shell.env.update_or_add(
                    name,
                    value,
                    |var| {
                        if self.unexport {
                            var.unexport();
                        } else {
                            var.export();
                        }
                        Ok(())
                    },
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;
            }
        }

        Ok(builtins::ExitCode::Success)
    }
}

fn display_all_exported_vars(context: &commands::ExecutionContext<'_>) -> Result<(), error::Error> {
    // Enumerate variables, sorted by key.
    for (name, variable) in context.shell.env.iter().sorted_by_key(|v| v.0) {
        if variable.is_exported() {
            let value = variable.value().try_get_cow_str(context.shell);
            if let Some(value) = value {
                writeln!(context.stdout(), "declare -x {name}=\"{value}\"")?;
            } else {
                writeln!(context.stdout(), "declare -x {name}")?;
            }
        }
    }

    Ok(())
}
