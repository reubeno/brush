use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables,
};

/// Add or update exported shell variables.
#[derive(Parser)]
pub(crate) struct ExportCommand {
    /// Use indexed arrays for assignments and filter listings to indexed arrays.
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
    // N.B. These are skipped by clap, but filled in by `impl_parse_args!`.
    #[clap(skip)]
    declarations: Vec<brush_core::CommandArg>,
}

brush_clap_adapter::impl_parse_args!(ExportCommand, declarations = declarations);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ExportCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.declarations.is_empty() {
            // With `-f`, `-a` selects functions that are indexed arrays: none.
            if !(self.make_indexed_array && self.names_are_functions) {
                display_all_exported_vars(&context, self.make_indexed_array)?;
            }
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
                // A word that expanded into an assignment (`export ${v}=x`): the
                // parser can't see it as one, so it is split here, as bash does.
                else if let Some((name, value)) = s.split_once('=') {
                    let (name, append) = name
                        .strip_suffix('+')
                        .map_or((name, false), |name| (name, true));
                    if !env::valid_variable_name(name) {
                        return Self::invalid_identifier(context, s);
                    }

                    let value = variables::ShellValueLiteral::Scalar(value.to_owned());
                    return self.assign_and_mark(context, name, value, append);
                } else if !env::valid_variable_name(s) {
                    return Self::invalid_identifier(context, s);
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported.
                else if let Some((_, variable)) = context.shell.env_mut().get_mut(s) {
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                } else if !self.unexport {
                    // A bare name records the export attribute, not an array type,
                    // even with `-a`. Later assignments determine the value's type.
                    let mut variable = variables::ShellVariable::new(variables::ShellValue::Unset(
                        variables::ShellValueUnsetType::Untyped,
                    ));
                    variable.export();
                    context
                        .shell
                        .env_mut()
                        .add(s, variable, EnvironmentScope::Global)?;
                }
            }
            brush_core::CommandArg::Assignment(assignment) => {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(name) => name,
                    ast::AssignmentName::ArrayElementName(name, index) => {
                        return Self::invalid_identifier(context, &format!("{name}[{index}]"));
                    }
                };

                let value = match &assignment.value {
                    ast::AssignmentValue::Scalar(s) => {
                        variables::ShellValueLiteral::Scalar(s.flatten())
                    }
                    ast::AssignmentValue::Array(a) => {
                        variables::ShellValueLiteral::Array(variables::ArrayLiteral::new(
                            a.iter()
                                .map(|(k, v)| (k.as_ref().map(|k| k.flatten()), v.flatten()))
                                .collect(),
                        ))
                    }
                };

                return self.assign_and_mark(context, name, value, assignment.append);
            }
        }

        Ok(ExecutionResult::success())
    }

    /// Reports `word` as an invalid name, as bash does, and returns the failure.
    fn invalid_identifier(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        word: &str,
    ) -> Result<ExecutionResult, brush_core::Error> {
        writeln!(
            context.stderr(),
            "{}: `{word}': not a valid identifier",
            context.command_name
        )?;
        Ok(ExecutionResult::general_error())
    }

    /// Assigns `value` to `name`, appending to it for `name+=value`, and marks
    /// the variable exported (or not, for `-n`).
    fn assign_and_mark(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
        value: variables::ShellValueLiteral,
        append: bool,
    ) -> Result<ExecutionResult, brush_core::Error> {
        if self.make_indexed_array {
            return self.assign_indexed_and_mark(context, name, value, append);
        }

        // `export name+=value` appends to the existing value, exactly like a
        // bare `name+=value`. update_or_add always replaces, so when the
        // variable already exists honor the append here. A missing variable
        // falls through: appending to nothing is a plain assignment.
        if append && let Some((_, variable)) = context.shell.env_mut().get_mut(name) {
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
        Ok(ExecutionResult::success())
    }

    /// Applies indexed assignment semantics without changing bare-name export behavior.
    fn assign_indexed_and_mark(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
        value: variables::ShellValueLiteral,
        append: bool,
    ) -> Result<ExecutionResult, brush_core::Error> {
        use variables::{ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable};

        let assign = |variable: &mut ShellVariable| -> Result<(), brush_core::Error> {
            // Bash applies the export attribute even when this assignment fails.
            if self.unexport {
                variable.unexport();
            } else {
                variable.export();
            }

            if variable.is_readonly() {
                return Err(brush_core::ErrorKind::ReadonlyVariable.into());
            }

            match variable.value() {
                ShellValue::AssociativeArray(_)
                | ShellValue::Unset(ShellValueUnsetType::AssociativeArray) => {
                    return Err(
                        brush_core::ErrorKind::ConvertingAssociativeArrayToIndexedArray.into(),
                    );
                }
                ShellValue::Unset(_) => {
                    // Assigning an array literal gives an untyped unset variable
                    // its indexed type without materializing an empty element zero.
                    let value = match value {
                        ShellValueLiteral::Scalar(s) => {
                            ShellValueLiteral::Array(variables::ArrayLiteral::new(vec![(None, s)]))
                        }
                        value => value,
                    };
                    return variable.assign(value, append);
                }
                _ => variable.convert_to_indexed_array()?,
            }

            variable.assign(value, append)
        };

        let result = if let Some((_, variable)) = context.shell.env_mut().get_mut(name) {
            assign(variable)
        } else {
            let mut variable =
                ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::IndexedArray));
            assign(&mut variable).and_then(|()| {
                context
                    .shell
                    .env_mut()
                    .add(name, variable, EnvironmentScope::Global)
            })
        };

        match result {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(error) => {
                let message = match error.kind() {
                    brush_core::ErrorKind::ReadonlyVariable => "readonly variable",
                    brush_core::ErrorKind::ConvertingAssociativeArrayToIndexedArray => {
                        "cannot convert associative to indexed array"
                    }
                    _ => return Err(error),
                };
                writeln!(
                    context.stderr(),
                    "{}: {name}: {message}",
                    context.command_name
                )?;
                Ok(ExecutionResult::general_error())
            }
        }
    }
}

fn display_all_exported_vars(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    indexed_arrays_only: bool,
) -> Result<(), brush_core::Error> {
    // Enumerate variables, sorted by key.
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if variable.is_exported() {
            if indexed_arrays_only {
                if !matches!(
                    variable.value(),
                    variables::ShellValue::IndexedArray(_)
                        | variables::ShellValue::Unset(
                            variables::ShellValueUnsetType::IndexedArray
                        )
                ) {
                    continue;
                }

                let flags = variable.attribute_flags(context.shell);
                let separator = if matches!(variable.value(), variables::ShellValue::Unset(_)) {
                    ""
                } else {
                    "="
                };
                let value = variable
                    .value()
                    .format(variables::FormatStyle::DeclarePrint, context.shell)?;
                writeln!(
                    context.stdout(),
                    "declare -{flags} {name}{separator}{value}"
                )?;
                continue;
            }

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
