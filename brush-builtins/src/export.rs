use brush_core::{write_async, writeln_async};
use itertools::Itertools;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{self, EnvironmentScope},
    error,
    parser::ast,
    variables,
};

/// Add or update exported shell variables.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
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
            // With `-f`, `-a` selects functions that are indexed arrays: none.
            if !(self.make_indexed_array && self.names_are_functions) {
                display_all_exported_vars(&context, self.make_indexed_array).await?;
            }
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
                if self.names_are_functions {
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
                    return self.assign_and_mark(context, name, value, append).await;
                } else if !env::valid_variable_name(s) {
                    return Self::invalid_identifier(context, s).await;
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported. For subscripted namerefs (e.g., ref→arr[1]), bash rejects the
                // target as "not a valid identifier" — export/unexport only applies to
                // whole variables. For circular namerefs, bash emits a warning and skips.
                else {
                    // Check for circular namerefs upfront so we can emit a warning
                    // (env_mut().get_mut() silently swallows the resolution error).
                    if let Err(err) = context.shell.env().resolve_nameref(s)
                        && matches!(err.kind(), error::ErrorKind::CircularNameReference(_))
                    {
                        writeln_async!(
                            context.stderr_async(),
                            "{}: warning: {err}",
                            context.command_name
                        )
                        .await?;
                    } else if let Some(mut resolved) = context.shell.env_mut().get_mut(s) {
                        if resolved.has_subscript() {
                            // Resolve the nameref to get the full target string for the error.
                            let target = context
                                .shell
                                .env()
                                .resolve_nameref_to_name(s)
                                .unwrap_or_else(|_| s.to_owned());
                            writeln_async!(
                                context.stderr_async(),
                                "{}: `{target}': not a valid identifier",
                                context.command_name
                            )
                            .await?;
                        } else if self.unexport {
                            resolved.base_var_mut().unexport();
                        } else {
                            resolved.base_var_mut().export();
                        }
                    } else if !self.unexport {
                        // A bare name records the export attribute, not an array type,
                        // even with `-a`. Later assignments determine the value's type.
                        let mut variable = variables::ShellVariable::new(
                            variables::ShellValue::Unset(variables::ShellValueUnsetType::Untyped),
                        );
                        variable.export();
                        context
                            .shell
                            .env_mut()
                            .add(s, variable, EnvironmentScope::Global)?;
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

                return self
                    .assign_and_mark(context, name, value, assignment.append)
                    .await;
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
    /// the variable exported (or not, for `-n`). With `-a`, the variable is first
    /// converted to an indexed array, as `declare -a` does.
    async fn assign_and_mark(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
        value: variables::ShellValueLiteral,
        append: bool,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // The assignment lands on the nameref's target, which may not exist yet.
        let target = context
            .shell
            .env()
            .resolve_nameref_or_default(name)
            .name()
            .to_owned();
        let mut fresh = variables::ShellVariable::new(variables::ShellValue::Unset(
            variables::ShellValueUnsetType::Untyped,
        ));
        let mut existing = context.shell.env_mut().get_mut(&target);
        let is_new = existing.is_none();
        let var = match existing.as_mut() {
            Some(resolved) => resolved.base_var_mut(),
            None => &mut fresh,
        };

        // Bash applies the export attribute even when the assignment fails.
        if self.unexport {
            var.unexport();
        } else {
            var.export();
        }

        let converted = if self.make_indexed_array {
            var.convert_to_indexed_array(None)
        } else {
            Ok(())
        };
        let result = converted.and_then(|()| var.assign(value, append));

        if is_new {
            context
                .shell
                .env_mut()
                .add(&target, fresh, EnvironmentScope::Global)?;
        }

        let Err(error) = result else {
            return Ok(ExecutionResult::success());
        };
        let message = match error.kind() {
            brush_core::ErrorKind::ReadonlyVariable => "readonly variable",
            brush_core::ErrorKind::ConvertingAssociativeArrayToIndexedArray => {
                "cannot convert associative to indexed array"
            }
            _ => return Err(error),
        };
        // Bash names the builtin only when the `-a` conversion is what failed.
        if self.make_indexed_array {
            write_async!(context.stderr_async(), "{}: ", context.command_name).await?;
        }
        writeln_async!(context.stderr_async(), "{name}: {message}").await?;
        Ok(ExecutionResult::general_error())
    }
}

async fn display_all_exported_vars(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    indexed_arrays_only: bool,
) -> Result<(), brush_core::Error> {
    // Enumerate variables, sorted by key, in `declare -p` form.
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if !variable.is_exported() {
            continue;
        }
        // Resolve once: a dynamic value can change between reads.
        let value = variable.resolve_value(context.shell);
        if indexed_arrays_only && !value.is_indexed_array() {
            continue;
        }
        let flags = variable.attribute_flags_for_value(&value);
        let separator = if matches!(value, variables::ShellValue::Unset(_)) {
            ""
        } else {
            "="
        };
        writeln_async!(
            context.stdout_async(),
            "declare -{flags} {name}{separator}{}",
            value.format(variables::FormatStyle::DeclarePrint, context.shell)?
        )
        .await?;
    }

    Ok(())
}
