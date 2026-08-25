//! The `export` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(ExportCommand);

use brush_core::{
    ExecutionExitCode, ExecutionResult,
    env::{EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables,
};
use itertools::Itertools;
use std::io::Write;

impl ExportCommand {
    fn process_decl(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        decl: &brush_core::CommandArg,
    ) -> Result<ExecutionResult, brush_core::Error> {
        match decl {
            brush_core::CommandArg::String(s) => {
                // See if this is supposed to be a function name.
                if self.names_are_functions {
                    // Try to find the function already present; if we find it, then mark it
                    // exported.
                    if let Some(func) = context.shell.func_mut(s) {
                        if self.unexport {
                            func.unexport();
                        } else {
                            func.export();
                        }
                    } else {
                        writeln!(context.stderr(), "{s}: not a function")?;
                        return Ok(ExecutionExitCode::InvalidUsage.into());
                    }
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported.
                else if let Some((_, variable)) = context.shell.env_mut().get_mut(s) {
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                }
            }
            brush_core::CommandArg::Assignment(assignment) => {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(name) => name,
                    ast::AssignmentName::ArrayElementName(_, _) => {
                        writeln!(context.stderr(), "not a valid variable name")?;
                        return Ok(ExecutionExitCode::InvalidUsage.into());
                    }
                };

                let value = match &assignment.value {
                    ast::AssignmentValue::Scalar(s) => {
                        variables::ShellValueLiteral::Scalar(s.flatten())
                    }
                    ast::AssignmentValue::Array(a) => {
                        variables::ShellValueLiteral::Array(variables::ArrayLiteral(
                            a.iter()
                                .map(|(k, v)| (k.as_ref().map(|k| k.flatten()), v.flatten()))
                                .collect(),
                        ))
                    }
                };

                // `export name+=value` appends to the existing value, exactly like a
                // bare `name+=value`. update_or_add always replaces, so when the
                // variable already exists honor the append here. A missing variable
                // falls through: appending to nothing is a plain assignment.
                if assignment.append
                    && let Some((_, variable)) = context.shell.env_mut().get_mut(name)
                {
                    variable.assign(value, true)?;
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                    return Ok(ExecutionResult::success());
                }

                // Update the variable with the provided value and then mark it exported.
                context.shell.env_mut().update_or_add(
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

        Ok(ExecutionResult::success())
    }
}

pub(super) fn display_all_exported_vars(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<(), brush_core::Error> {
    // Enumerate variables, sorted by key.
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &ExportCommand,
    mut context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.declarations.is_empty() {
        display_all_exported_vars(&context)?;
        return Ok(ExecutionResult::success());
    }

    let mut result = ExecutionResult::success();
    for decl in &command.declarations {
        let current_result = command.process_decl(&mut context, decl)?;
        if !current_result.is_success() {
            result = current_result;
        }
    }

    Ok(result)
}
