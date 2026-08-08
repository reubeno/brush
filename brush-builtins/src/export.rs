use clap::Parser;
use itertools::Itertools;
use std::{borrow::Cow, io::Write};

use brush_core::{
    ExecutionResult, builtins,
    env::{EnvironmentLookup, EnvironmentScope},
    expansion::AssignmentTarget,
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

/// An export operand whose structure is fully resolved and which needs no further expansion.
enum PreparedExport {
    /// The operand only names a variable or function.
    Name(String),
    /// The operand assigns a value.
    Assignment(ast::Assignment),
}

impl PreparedExport {
    /// Returns the text `export` echoes as an extra `set -x` trace line for this operand, or
    /// `None` if this operand is not echoed.
    ///
    /// Only a scalar assignment to a whole variable is echoed: a bare name assigns nothing, an
    /// array element is not a valid `export` target, and a compound assignment is traced by a
    /// shell in a different form that is not reproduced here.
    fn render_traced_assignment(&self) -> Option<String> {
        let Self::Assignment(assignment) = self else {
            return None;
        };
        if !matches!(assignment.name, ast::AssignmentName::VariableName(_))
            || !matches!(assignment.value, ast::AssignmentValue::Scalar(_))
        {
            return None;
        }

        let op = if assignment.append { "+=" } else { "=" };
        Some(std::format!(
            "{}{op}{}",
            assignment.name,
            assignment_value(assignment)
        ))
    }
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

        // Resolve every operand against the environment as it existed before the command, so that
        // later operands cannot observe variables assigned by earlier ones.
        let mut prepared_exports = Vec::with_capacity(self.declarations.len());
        for declaration in &self.declarations {
            prepared_exports.push(self.prepare_export(&mut context, declaration).await?);
        }

        // `export` echoes each of its assignments as an extra trace line, on top of the trace the
        // interpreter already emitted for the invocation itself.
        context
            .trace_extra_lines(
                prepared_exports
                    .iter()
                    .filter_map(PreparedExport::render_traced_assignment),
            )
            .await;

        let mut result = ExecutionResult::success();
        for export in &prepared_exports {
            let current_result = self.apply_export(&mut context, export)?;
            if !current_result.is_success() {
                result = current_result;
            }
        }

        Ok(result)
    }
}

impl ExportCommand {
    /// Prepares one export operand and returns a representation that needs no further shell
    /// expansion.
    ///
    /// A [`brush_core::CommandArg::Assignment`] had its words expanded by the interpreter, so only
    /// its subscripts remain to be resolved. A [`brush_core::CommandArg::String`] may still hold
    /// assignment syntax produced by quoting or by an expansion; that syntax is recognized here,
    /// and its value is deliberately left verbatim rather than expanded a second time.
    ///
    /// # Arguments
    ///
    /// * `context` - The shell context used for expansion, parser options, and target lookup.
    /// * `declaration` - The operand to prepare.
    async fn prepare_export(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
    ) -> Result<PreparedExport, brush_core::Error> {
        let assignment = match declaration {
            brush_core::CommandArg::Assignment(assignment) => Cow::Borrowed(assignment),
            brush_core::CommandArg::String(operand) => {
                match brush_parser::word::parse_scalar_assignment(
                    operand,
                    &context.shell.parser_options(),
                ) {
                    Ok(assignment) => Cow::Owned(assignment),
                    Err(_) => return Ok(PreparedExport::Name(operand.clone())),
                }
            }
        };

        // `export` has no array-typing options, so the target keeps whatever kind it already has.
        let target = context
            .shell
            .existing_array_kind(assignment.name.base_name(), EnvironmentLookup::Anywhere)
            .unwrap_or(AssignmentTarget::IndexedArray);
        let assignment = context
            .shell
            .resolve_assignment_subscripts(&context.params, &assignment, target)
            .await?;
        Ok(PreparedExport::Assignment(assignment))
    }

    /// Applies one prepared export operand and returns its command-level execution result. Expansion
    /// failures cannot occur here because preparation completed before any operands were applied.
    ///
    /// # Arguments
    ///
    /// * `context` - The shell context whose functions or variables are updated.
    /// * `export` - A fully prepared name or assignment to apply.
    fn apply_export(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        export: &PreparedExport,
    ) -> Result<ExecutionResult, brush_core::Error> {
        match export {
            PreparedExport::Name(name) => {
                // See if this is supposed to be a function name.
                if self.names_are_functions {
                    // Try to find the function already present; if we find it, then mark it
                    // exported.
                    if let Some(func) = context.shell.func_mut(name) {
                        if self.unexport {
                            func.unexport();
                        } else {
                            func.export();
                        }
                    } else {
                        writeln!(
                            context.stderr(),
                            "{}: {name}: not a function",
                            context.command_name
                        )?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported.
                else if let Some((_, variable)) = context.shell.env_mut().get_mut(name) {
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                }
            }
            PreparedExport::Assignment(assignment) => {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(name) => name,
                    // `export` names whole variables; an array element is not a valid target.
                    ast::AssignmentName::ArrayElementName(name, index) => {
                        writeln!(
                            context.stderr(),
                            "{}: `{name}[{index}]': not a valid identifier",
                            context.command_name,
                        )?;
                        return Ok(ExecutionResult::general_error());
                    }
                };

                let value = assignment_value(assignment);

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

fn assignment_value(assignment: &ast::Assignment) -> variables::ShellValueLiteral {
    match &assignment.value {
        ast::AssignmentValue::Scalar(value) => {
            variables::ShellValueLiteral::Scalar(value.flatten())
        }
        ast::AssignmentValue::Array(values) => {
            variables::ShellValueLiteral::Array(variables::ArrayLiteral(
                values
                    .iter()
                    .map(|(key, value)| (key.as_ref().map(|key| key.flatten()), value.flatten()))
                    .collect(),
            ))
        }
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
