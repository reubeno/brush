use std::borrow::Cow;

use clap::Parser;

use brush_core::{
    ExecutionResult, Shell, ShellValue, builtins, env::VarNameExt, variables::ShellValueUnsetType,
};

/// Unset a variable.
#[derive(Parser)]
pub(crate) struct UnsetCommand {
    #[clap(flatten)]
    name_interpretation: UnsetNameInterpretation,

    /// Names of variables to unset.
    names: Vec<String>,
}

#[derive(Parser)]
#[clap(group = clap::ArgGroup::new("name-interpretation").multiple(false).required(false))]
pub(crate) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    #[arg(short = 'f', group = "name-interpretation")]
    shell_functions: bool,

    /// Treat each name as a shell variable.
    #[arg(short = 'v', group = "name-interpretation")]
    shell_variables: bool,

    /// Treat each name as a name reference.
    #[arg(short = 'n', group = "name-interpretation")]
    name_references: bool,
}

impl UnsetNameInterpretation {
    pub const fn unspecified(&self) -> bool {
        !self.shell_functions && !self.shell_variables && !self.name_references
    }
}

impl builtins::Command for UnsetCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let unspecified = self.name_interpretation.unspecified();

        #[expect(clippy::needless_continue)]
        for name in &self.names {
            if self.name_interpretation.name_references {
                // `unset -n`: removes the nameref variable itself, not its target.
                // Per bash semantics, `unset -n` on a non-nameref variable is a
                // silent no-op — the variable is left untouched.
                let is_nameref = context
                    .shell
                    .env()
                    .lookup(name.direct())
                    .get_direct()
                    .is_some_and(|(_, v)| v.is_treated_as_nameref());
                if is_nameref {
                    context.shell.env_mut().unset(name.direct())?;
                }
                // `unset -n` never touches functions or array elements — it
                // operates only on nameref variables. Skip the rest of the loop.
                continue;
            }

            if unspecified || self.name_interpretation.shell_variables {
                // Try to parse the name as a parameter. If we can't, don't bail; it may not be a
                // valid variable name/parameter but could still be a function name.
                if let Ok(parameter) =
                    brush_parser::word::parse_parameter(name, &context.shell.parser_options())
                {
                    let result = match parameter {
                        brush_parser::word::Parameter::Positional(_) => continue,
                        brush_parser::word::Parameter::Special(_) => continue,
                        brush_parser::word::Parameter::Named(name) => {
                            context.shell.env_mut().unset(name.as_str())?.is_some()
                        }
                        brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                            unset_array_index(context.shell, name.as_str(), index.as_str())?
                        }
                        brush_parser::word::Parameter::NamedWithAllIndices {
                            name: _,
                            concatenate: _,
                        } => continue,
                    };

                    if result {
                        continue;
                    }
                }
            }

            // TODO(unset): Deal with readonly functions
            if unspecified || self.name_interpretation.shell_functions {
                if context.shell.undefine_func(name) {
                    continue;
                }
            }
        }

        Ok(ExecutionResult::success())
    }
}

fn unset_array_index(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    name: &str,
    index: &str,
) -> Result<bool, brush_core::Error> {
    // Resolve the nameref once upfront to avoid double resolution.
    // Circular namerefs silently fall back to the identity name (bash doesn't
    // warn in the unset-array-element path).
    let resolved = shell.env().resolve_nameref_or_default(name);

    // Check if the resolved target is an associative array (use lookup with the
    // already-resolved name to avoid redundant nameref resolution).
    let is_assoc_array = if let Some((_, var)) = shell.env().lookup(&resolved).get_direct() {
        matches!(
            var.value(),
            ShellValue::AssociativeArray(_)
                | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
        )
    } else {
        false
    };

    // Compute which index we should actually use. For indexed arrays, we need to evaluate
    // the index string as an arithmetic expression first.
    let index_to_use: Cow<'_, str> = if is_assoc_array {
        index.into()
    } else {
        // First evaluate the index expression.
        let index_as_expr = brush_parser::arithmetic::parse(index)?;
        let evaluated_index = shell.eval_arithmetic(&index_as_expr)?;
        evaluated_index.to_string().into()
    };

    // Use lookup_mut with the already-resolved name to avoid redundant nameref resolution.
    if let Some((_, var)) = shell.env_mut().lookup_mut(&resolved).get_direct() {
        var.unset_index(index_to_use.as_ref())
    } else {
        Ok(false)
    }
}
