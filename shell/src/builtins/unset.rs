use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser)]
pub(crate) struct UnsetCommand {
    #[clap(flatten)]
    name_interpretation: UnsetNameInterpretation,

    names: Vec<String>,
}

#[derive(Parser)]
#[clap(group = clap::ArgGroup::new("name-interpretation").multiple(false).required(false))]
#[allow(clippy::struct_field_names)]
pub(crate) struct UnsetNameInterpretation {
    #[arg(
        short = 'f',
        group = "name-interpretation",
        help = "treat each name as a shell function"
    )]
    names_are_shell_functions: bool,

    #[arg(
        short = 'v',
        group = "name-interpretation",
        help = "treat each name as a shell variable"
    )]
    names_are_shell_variables: bool,

    #[arg(
        short = 'n',
        group = "name-interpretation",
        help = "treat each name as a name reference"
    )]
    names_are_name_references: bool,
}

impl UnsetNameInterpretation {
    pub fn unspecified(&self) -> bool {
        !self.names_are_shell_functions
            && !self.names_are_shell_variables
            && !self.names_are_name_references
    }
}

#[async_trait::async_trait]
impl BuiltinCommand for UnsetCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        //
        // TODO: implement nameref
        //
        if self.name_interpretation.names_are_name_references {
            return crate::error::unimp("unset: name references are not yet implemented");
        }

        let unspecified = self.name_interpretation.unspecified();

        for name in &self.names {
            if unspecified || self.name_interpretation.names_are_shell_variables {
                let parameter =
                    parser::word::parse_parameter(name, &context.shell.parser_options())?;

                let result = match parameter {
                    parser::word::Parameter::Positional(_) => continue,
                    parser::word::Parameter::Special(_) => continue,
                    parser::word::Parameter::Named(name) => {
                        context.shell.env.unset(name.as_str())?
                    }
                    parser::word::Parameter::NamedWithIndex { name, index } => {
                        // TODO: Evaluate index?
                        context
                            .shell
                            .env
                            .unset_index(name.as_str(), index.as_str())?
                    }
                    parser::word::Parameter::NamedWithAllIndices {
                        name: _,
                        concatenate: _,
                    } => continue,
                };

                if result {
                    continue;
                }
            }

            // TODO: Check if functions can be readonly.
            if unspecified || self.name_interpretation.names_are_shell_functions {
                if context.shell.funcs.remove(name).is_some() {
                    continue;
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
