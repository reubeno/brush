//! `unset` builtin: `UnsetCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Parser;
use std::borrow::Cow;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;
use brush_core::Shell;

/// How the names passed to `unset` should be interpreted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NameInterpretation {
    Functions,
    Variables,
    NameRefs,
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

/// Unset a variable.
pub(crate) struct UnsetCommand {
    pub(super) name_interpretation: Option<NameInterpretation>,
    pub(super) names: Vec<String>,
}

/// How each provided name should be interpreted.
pub(super) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    pub(super) shell_functions: bool,

    /// Treat each name as a shell variable.
    pub(super) shell_variables: bool,

    /// Treat each name as a name reference.
    pub(super) name_references: bool,
}

impl crate::args::BpafArgs for UnsetCommand {
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
}

impl FromArgs for UnsetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for UnsetCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
