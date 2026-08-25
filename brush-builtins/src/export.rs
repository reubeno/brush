use itertools::Itertools;
use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables,
};

const ID_NAMES_ARE_FUNCTIONS: &str = "names_are_functions";
const ID_UNEXPORT: &str = "unexport";
const ID_DISPLAY_EXPORTED_NAMES: &str = "display_exported_names";

/// Add or update exported shell variables.
pub(crate) struct ExportCommand {
    /// Names are treated as function names.
    names_are_functions: bool,

    /// Un-export the names.
    unexport: bool,

    /// Display all exported names.
    #[expect(dead_code)]
    display_exported_names: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by the parser, but filled in by the
    // SpecCommand trait.
    declarations: Vec<brush_core::CommandArg>,
}

impl builtins::SpecCommand for ExportCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_NAMES_ARE_FUNCTIONS,
            &['f'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Names are treated as function names.",
        )
        .arg(
            ID_UNEXPORT,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Un-export the names.",
        )
        .arg(
            ID_DISPLAY_EXPORTED_NAMES,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Display all exported names.",
        )
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            names_are_functions: matches.flag(ID_NAMES_ARE_FUNCTIONS),
            unexport: matches.flag(ID_UNEXPORT),
            display_exported_names: matches.flag(ID_DISPLAY_EXPORTED_NAMES),

            // N.B. Declarations are captured separately from options.
            declarations: Vec::new(),
        })
    }

    fn uses_declarations() -> bool {
        true
    }

    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }

    fn about() -> &'static str {
        "Add or update exported shell variables."
    }

    fn synopsis() -> &'static str {
        "[-fn] [NAME[=VALUE]]..."
    }

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
