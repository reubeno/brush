use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use brush_core::{
    ExecutionExitCode, ExecutionResult, builtins,
    env::{EnvironmentLookup, EnvironmentScope},
    error,
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
    type State = ();
    type SharedState = ();
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.declarations.is_empty() {
            let output = display_all_exported_vars(&context)?;
            if !output.is_empty() {
                if let Some(mut stdout) = context.stdout_async() {
                    stdout.write_all(&output).await?;
                    stdout.flush().await?;
                } else {
                    context.stdout().write_all(&output)?;
                    context.stdout().flush()?;
                }
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
    #[expect(clippy::too_many_lines)]
    fn process_decl(
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
                        writeln!(context.stderr(), "{s}: not a function")?;
                        return Ok(ExecutionExitCode::InvalidUsage.into());
                    }
                }
                // A word argument that *expanded* into an assignment (e.g.
                // `export ${var}=value`): the parser cannot classify it as an
                // assignment at parse time, so declaration utilities split it
                // at runtime, as bash does. `name+=value` appends.
                else if let Some((raw_name, value)) = s.split_once('=') {
                    let (name, append) = match raw_name.strip_suffix('+') {
                        Some(n) => (n, true),
                        None => (raw_name, false),
                    };
                    let valid = !name.is_empty()
                        && name
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                    if !valid {
                        writeln!(
                            context.stderr(),
                            "{}: `{s}': not a valid identifier",
                            context.command_name
                        )?;
                        return Ok(ExecutionResult::new(1));
                    }
                    let new_value = if append {
                        let existing = context
                            .shell
                            .env()
                            .get_str(name, context.shell)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        format!("{existing}{value}")
                    } else {
                        value.to_owned()
                    };
                    context.shell.env_mut().update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(new_value),
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
                        writeln!(context.stderr(), "{}: warning: {err}", context.command_name)?;
                    } else if let Some(mut resolved) = context.shell.env_mut().get_mut(s) {
                        if resolved.has_subscript() {
                            // Resolve the nameref to get the full target string for the error.
                            let target = context
                                .shell
                                .env()
                                .resolve_nameref_to_name(s)
                                .unwrap_or_else(|_| s.to_owned());
                            writeln!(
                                context.stderr(),
                                "{}: `{target}': not a valid identifier",
                                context.command_name
                            )?;
                        } else if self.unexport {
                            resolved.base_var_mut().unexport();
                        } else {
                            resolved.base_var_mut().export();
                        }
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
) -> Result<Vec<u8>, brush_core::Error> {
    let mut output = Vec::new();

    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if variable.is_exported() {
            let value = variable.value().try_get_cow_str(context.shell);
            if let Some(value) = value {
                writeln!(output, "declare -x {name}=\"{value}\"")?;
            } else {
                writeln!(output, "declare -x {name}")?;
            }
        }
    }

    Ok(output)
}
