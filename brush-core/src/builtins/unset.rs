use std::borrow::Cow;

use clap::Parser;

use crate::{Shell, ShellValue, builtins, commands, error, variables::ShellValueUnsetType};

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
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        //
        // TODO: implement nameref
        //
        if self.name_interpretation.name_references {
            return crate::error::unimp("unset: name references are not yet implemented");
        }

        let unspecified = self.name_interpretation.unspecified();

        #[allow(clippy::needless_continue)]
        for name in &self.names {
            if unspecified || self.name_interpretation.shell_variables {
                let parameter =
                    brush_parser::word::parse_parameter(name, &context.shell.parser_options())?;

                let result = match parameter {
                    brush_parser::word::Parameter::Positional(_) => continue,
                    brush_parser::word::Parameter::Special(_) => continue,
                    brush_parser::word::Parameter::Named(name) => {
                        context.shell.env.unset(name.as_str())?.is_some()
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

            // TODO: Deal with readonly functions
            if unspecified || self.name_interpretation.shell_functions {
                if context.shell.funcs.remove(name).is_some() {
                    continue;
                }
            }
        }

        Ok(builtins::ExitCode::Success)
    }
}

fn unset_array_index(shell: &mut Shell, name: &str, index: &str) -> Result<bool, error::Error> {
    // First check to see if it's an associative array.
    let is_assoc_array = if let Some((_, var)) = shell.env.get(name) {
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

    // Now we can try to unset, and return the result.
    shell.env.unset_index(name, index_to_use.as_ref())
}
