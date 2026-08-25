use std::borrow::Cow;

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues, PositionalSpec};
use brush_core::{ExecutionResult, Shell, builtins};

/// How the names passed to `unset` should be interpreted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NameInterpretation {
    Functions,
    Variables,
    NameRefs,
}

const ID_FUNCTIONS: &str = "functions";
const ID_VARIABLES: &str = "variables";
const ID_NAME_REFS: &str = "name_refs";
const ID_NAMES: &str = "names";

/// Unset a variable.
pub(crate) struct UnsetCommand {
    name_interpretation: Option<NameInterpretation>,
    names: Vec<String>,
}

static UNSET_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(
            ID_FUNCTIONS,
            &['f'],
            &[],
            "Treat each name as a shell function.",
        ),
        ArgSpec::flag(
            ID_VARIABLES,
            &['v'],
            &[],
            "Treat each name as a shell variable.",
        ),
        ArgSpec::flag(
            ID_NAME_REFS,
            &['n'],
            &[],
            "Treat each name as a name reference.",
        ),
    ],
    positionals: &[PositionalSpec::many(ID_NAMES, "NAMES")],
};

impl builtins::SpecCommand for UnsetCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &UNSET_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        let selected = [
            values.flag(ID_FUNCTIONS),
            values.flag(ID_VARIABLES),
            values.flag(ID_NAME_REFS),
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count();

        if selected > 1 {
            return Err(builtins::BuiltinArgParseError {
                message: String::from("cannot use -f, -v and -n together"),
                help_request: false,
            });
        }

        let name_interpretation = if values.flag(ID_FUNCTIONS) {
            Some(NameInterpretation::Functions)
        } else if values.flag(ID_VARIABLES) {
            Some(NameInterpretation::Variables)
        } else if values.flag(ID_NAME_REFS) {
            Some(NameInterpretation::NameRefs)
        } else {
            None
        };

        Ok(Self {
            name_interpretation,
            names: values.positional_values(ID_NAMES).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Unset values and attributes of variables and functions."
    }

    fn synopsis() -> &'static str {
        "[-fvn] [NAMES]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        //
        // TODO(nameref): implement nameref
        //
        if self.name_interpretation == Some(NameInterpretation::NameRefs) {
            return brush_core::error::unimp("unset: name references are not yet implemented");
        }

        let unspecified = self.name_interpretation.is_none();

        #[expect(clippy::needless_continue)]
        for name in &self.names {
            if unspecified || self.name_interpretation == Some(NameInterpretation::Variables) {
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
            if unspecified || self.name_interpretation == Some(NameInterpretation::Functions) {
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
    // First check to see if it's an associative array.
    let is_assoc_array = shell
        .env()
        .get(name)
        .is_some_and(|(_, var)| var.value().is_associative_array());

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
    shell.env_mut().unset_index(name, index_to_use.as_ref())
}
