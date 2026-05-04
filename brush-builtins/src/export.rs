use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables::{self, ShellValue, ShellValueUnsetType, ShellVariable},
};

/// Add or update exported shell variables.
#[derive(Parser)]
pub(crate) struct ExportCommand {
    /// Mark names as indexed arrays (combined with the export attribute).
    #[arg(short = 'a')]
    make_indexed_array: bool,

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
                    if self.make_indexed_array {
                        variable.convert_to_indexed_array()?;
                    }
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                }
                // If `-a` was passed and the name doesn't yet exist, create it as an unset
                // indexed array with the export attribute (mirrors `declare -ax NAME`). This
                // is what bash does for `export -a NAME` and what `mise activate bash` relies
                // on when seeding `chpwd_functions`.
                //
                // But `-n` (unexport) on a name that doesn't exist yet is a no-op in bash --
                // there's nothing to unexport, so don't materialize a new unset array var just
                // because `-a` was also given. Only create the var on the exporting path.
                else if self.make_indexed_array && !self.unexport {
                    let mut var =
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::IndexedArray));
                    var.export();
                    context
                        .shell
                        .env_mut()
                        .add(s.clone(), var, EnvironmentScope::Global)?;
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
                //
                // When `-a` is also set, convert to an indexed array first (mirrors
                // bash's `declare -ax foo+=x`) so the append lands on an array rather
                // than silently ignoring `-a` on this path.
                if assignment.append
                    && let Some((_, variable)) = context.shell.env_mut().get_mut(name)
                {
                    if self.make_indexed_array {
                        variable.convert_to_indexed_array()?;
                    }
                    variable.assign(value, true)?;
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                    return Ok(ExecutionResult::success());
                }

                // Update the variable with the provided value and then mark it exported.
                // `update_or_add` assigns the scalar value first and only then invokes this
                // updater closure, so when `-a` is set we convert to an indexed array *after*
                // the assignment lands -- `convert_to_indexed_array` takes whatever scalar was
                // just assigned and re-seeds it as index 0. Net effect: `export -a foo=x` ends
                // up as `foo=([0]="x")`, matching bash's `declare -ax foo=([0]="x")`, but via a
                // convert-after-assign step, not a before-assign one.
                context.shell.env_mut().update_or_add(
                    name,
                    value,
                    |var| {
                        if self.make_indexed_array {
                            var.convert_to_indexed_array()?;
                        }
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
