use bpaf::Parser;
use std::borrow::Cow;

use brush_core::{ExecutionResult, Shell, builtins};

/// How the names passed to `unset` should be interpreted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NameInterpretation {
    Functions,
    Variables,
    NameRefs,
}

/// Unset a variable.
pub(crate) struct UnsetCommand {
    name_interpretation: Option<NameInterpretation>,
    names: Vec<String>,
}

impl builtins::Command for UnsetCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        let functions = bpaf::short('f')
            .help("Treat each name as a shell function.")
            .req_flag(NameInterpretation::Functions);
        let variables = bpaf::short('v')
            .help("Treat each name as a shell variable.")
            .req_flag(NameInterpretation::Variables);
        let name_refs = bpaf::short('n')
            .help("Treat each name as a name reference.")
            .req_flag(NameInterpretation::NameRefs);

        let name_interpretation = bpaf::construct!([functions, variables, name_refs]).optional();

        let names = bpaf::positional::<String>("NAMES")
            .help("Names of variables to unset.")
            .many();

        bpaf::construct!(UnsetCommand {
            name_interpretation,
            names,
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
