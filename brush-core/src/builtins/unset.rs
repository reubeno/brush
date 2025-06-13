use clap::Parser;

use crate::{builtins, commands};

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
                        // First evaluate the index expression.
                        let index_as_expr = brush_parser::arithmetic::parse(index.as_str())?;
                        let evaluated_index = context.shell.eval_arithmetic(&index_as_expr)?;

                        context
                            .shell
                            .env
                            .unset_index(name.as_str(), evaluated_index.to_string().as_str())?
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
