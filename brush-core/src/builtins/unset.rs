use clap::Parser;

use crate::{builtin, commands};

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
    #[arg(
        short = 'f',
        group = "name-interpretation",
        help = "treat each name as a shell function"
    )]
    shell_functions: bool,

    #[arg(
        short = 'v',
        group = "name-interpretation",
        help = "treat each name as a shell variable"
    )]
    shell_variables: bool,

    #[arg(
        short = 'n',
        group = "name-interpretation",
        help = "treat each name as a name reference"
    )]
    name_references: bool,
}

impl UnsetNameInterpretation {
    pub fn unspecified(&self) -> bool {
        !self.shell_functions && !self.shell_variables && !self.name_references
    }
}

#[async_trait::async_trait]
impl builtin::Command for UnsetCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        //
        // TODO: implement nameref
        //
        if self.name_interpretation.name_references {
            return crate::error::unimp("unset: name references are not yet implemented");
        }

        let unspecified = self.name_interpretation.unspecified();

        for name in &self.names {
            if unspecified || self.name_interpretation.shell_variables {
                let parameter =
                    brush_parser::word::parse_parameter(name, &context.shell.parser_options())?;

                let result = match parameter {
                    brush_parser::word::Parameter::Positional(_) => continue,
                    brush_parser::word::Parameter::Special(_) => continue,
                    brush_parser::word::Parameter::Named(name) => {
                        context.shell.env.unset(name.as_str())?
                    }
                    brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                        // TODO: Evaluate index?
                        context
                            .shell
                            .env
                            .unset_index(name.as_str(), index.as_str())?
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

        Ok(builtin::ExitCode::Success)
    }
}
