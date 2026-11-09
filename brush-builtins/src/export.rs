use brush_core::writeln_async;
use itertools::Itertools;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables,
};

/// Add or update exported shell variables.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
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
    // N.B. These are skipped by winnow-args, but filled in by `impl_parse_args!`.
    #[arg(skip)]
    declarations: Vec<brush_core::CommandArg>,
}

brush_winnow_args_adapter::impl_parse_args!(ExportCommand, declarations = declarations);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ExportCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.declarations.is_empty() {
            display_all_exported_vars(&context).await?;
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for decl in &self.declarations {
            let current_result = self.process_decl(&mut context, decl).await?;
            if !current_result.is_success() {
                result = current_result;
            }
        }

        Ok(result)
    }
}

impl ExportCommand {
    async fn process_decl(
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
                        writeln_async!(context.stderr_async(), "{s}: not a function").await?;
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
                        return Self::invalid_identifier(context, s).await;
                    }

                    let value = variables::ShellValueLiteral::Scalar(value.to_owned());
                    self.assign_and_mark(context, name, value, append)?;
                } else if !env::valid_variable_name(s) {
                    return Self::invalid_identifier(context, s).await;
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
                    ast::AssignmentName::ArrayElementName(name, index) => {
                        return Self::invalid_identifier(context, &format!("{name}[{index}]"))
                            .await;
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

                self.assign_and_mark(context, name, value, assignment.append)?;
            }
        }

        Ok(ExecutionResult::success())
    }

    /// Reports `word` as an invalid name, as bash does, and returns the failure.
    async fn invalid_identifier(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        word: &str,
    ) -> Result<ExecutionResult, brush_core::Error> {
        writeln_async!(
            context.stderr_async(),
            "{}: `{word}': not a valid identifier",
            context.command_name
        )
        .await?;
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
    ) -> Result<(), brush_core::Error> {
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
            return Ok(());
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
        )
    }
}

async fn display_all_exported_vars(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<(), brush_core::Error> {
    // Enumerate variables, sorted by key.
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if variable.is_exported() {
            let value = variable.value().try_get_cow_str(context.shell);
            if let Some(value) = value {
                writeln_async!(context.stdout_async(), "declare -x {name}=\"{value}\"").await?;
            } else {
                writeln_async!(context.stdout_async(), "declare -x {name}").await?;
            }
        }
    }

    Ok(())
}
