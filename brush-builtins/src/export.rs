use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{EnvironmentLookup, EnvironmentScope},
    parser::ast,
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
    declarations: Vec<brush_core::CommandArg>,
}

impl builtins::DeclarationCommand for ExportCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for ExportCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.declarations.is_empty() {
            display_all_exported_vars(&context)?;
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for decl in &self.declarations {
            let current_result = self.process_decl(&mut context, decl)?;
            if !current_result.is_success() {
                result = current_result;
            }
        }

        Ok(result)
    }
}

impl ExportCommand {
    /// Handles an expansion-formed `name=value` export argument (e.g.
    /// `export ${p}var=val` or `export "x=a=b"`), which reaches the builtin as
    /// an already-expanded string rather than a parsed assignment. bash
    /// re-parses declaration-builtin arguments as assignments. Returns `None`
    /// when `s` isn't an assignment form (a function name, or no `=`), so the
    /// caller falls through to the mark-existing-variable handling.
    fn try_export_string_assignment(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        s: &str,
    ) -> Result<Option<ExecutionResult>, brush_core::Error> {
        if self.names_are_functions {
            return Ok(None);
        }
        let Some((name, value)) = s.split_once('=') else {
            return Ok(None);
        };
        if !brush_core::env::valid_variable_name(name) {
            writeln!(
                context.stderr(),
                "{}: `{name}': not a valid identifier",
                context.command_name,
            )?;
            return Ok(Some(ExecutionExitCode::GeneralError.into()));
        }
        context.shell.env_mut().update_or_add(
            name,
            variables::ShellValueLiteral::Scalar(value.to_owned()),
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
        Ok(Some(ExecutionResult::success()))
    }

    fn process_decl(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        decl: &brush_core::CommandArg,
    ) -> Result<ExecutionResult, brush_core::Error> {
        match decl {
            brush_core::CommandArg::String(s) => {
                // An expansion-formed assignment (`export ${p}var=val`) arrives
                // as a string rather than a parsed assignment; handle it first.
                if let Some(result) = self.try_export_string_assignment(context, s)? {
                    return Ok(result);
                }

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
                        // bash returns 1 (general error), not 2, here.
                        return Ok(ExecutionExitCode::GeneralError.into());
                    }
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported. Subscripted-nameref targets (e.g., ref→arr[1]) are rejected
                // as "not a valid identifier". Circular namerefs emit a warning and skip.
                else {
                    // Single resolve — reuse the result for both the cycle/subscript
                    // check and the mutable lookup (no redundant chain walk).
                    let resolved = match context.shell.env().resolve_nameref(s) {
                        Ok(r) => r,
                        Err(fault) => {
                            context.shell.warn_nameref_fault(&fault)?;
                            return Ok(ExecutionResult::success());
                        }
                    };
                    if let Some(sub) = resolved.subscript() {
                        writeln!(
                            context.stderr(),
                            "{}: `{}[{sub}]': not a valid identifier",
                            context.command_name,
                            resolved.name(),
                        )?;
                    } else if let Some((_, var)) =
                        context.shell.env_mut().lookup_mut_resolved(&resolved).get()
                    {
                        if self.unexport {
                            var.unexport();
                        } else {
                            var.export();
                        }
                    }
                }
            }
            brush_core::CommandArg::Assignment(assignment) => {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(name) => name,
                    ast::AssignmentName::ArrayElementName(var_name, index) => {
                        // bash: `export: `arr[0]': not a valid identifier` (rc 1).
                        writeln!(
                            context.stderr(),
                            "{}: `{var_name}[{index}]': not a valid identifier",
                            context.command_name,
                        )?;
                        return Ok(ExecutionExitCode::GeneralError.into());
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
                if assignment.append {
                    let resolved = brush_core::env::ResolvedName::plain(name.as_str());
                    if let Some((_, variable)) =
                        context.shell.env_mut().lookup_mut_resolved(&resolved).get()
                    {
                        variable.assign(value, true)?;
                        if self.unexport {
                            variable.unexport();
                        } else {
                            variable.export();
                        }
                        return Ok(ExecutionResult::success());
                    }
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

fn display_all_exported_vars(
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
