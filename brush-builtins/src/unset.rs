//! The `unset` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
#[cfg(feature = "parser-bpaf")]
use self::bpaf::UnsetNameInterpretation;
#[cfg(feature = "parser-clap")]
use self::clap::UnsetNameInterpretation;
#[cfg(feature = "parser-usage")]
use self::usage::UnsetNameInterpretation;
arg_impl!(UnsetCommand);

use brush_core::{ExecutionResult, Shell};
use std::borrow::Cow;

impl UnsetNameInterpretation {
    pub const fn unspecified(&self) -> bool {
        !self.shell_functions && !self.shell_variables && !self.name_references
    }
}

pub(super) fn unset_array_index(
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &UnsetCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    //
    // TODO(nameref): implement nameref
    //
    if command.name_interpretation.name_references {
        return brush_core::error::unimp("unset: name references are not yet implemented");
    }

    let unspecified = command.name_interpretation.unspecified();

    #[expect(clippy::needless_continue)]
    for name in &command.names {
        if unspecified || command.name_interpretation.shell_variables {
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
        if unspecified || command.name_interpretation.shell_functions {
            if context.shell.undefine_func(name) {
                continue;
            }
        }
    }

    Ok(ExecutionResult::success())
}
