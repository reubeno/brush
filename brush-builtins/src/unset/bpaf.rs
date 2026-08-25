//! `unset` builtin: `UnsetCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;


/// How each provided name should be interpreted.
#[derive(Default)]
pub(crate) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    pub(super) shell_functions: bool,

    /// Treat each name as a shell variable.
    pub(super) shell_variables: bool,

    /// Treat each name as a name reference.
    pub(super) name_references: bool,
}

/// Unset a variable.
pub(crate) struct UnsetCommand {
    pub(super) name_interpretation: UnsetNameInterpretation,
    pub(super) names: Vec<String>,
}

impl crate::args::BpafArgs for UnsetCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        let shell_functions = bpaf::short('f')
            .help("Treat each name as a shell function.")
            .switch();
        let shell_variables = bpaf::short('v')
            .help("Treat each name as a shell variable.")
            .switch();
        let name_references = bpaf::short('n')
            .help("Treat each name as a name reference.")
            .switch();

        let name_interpretation = bpaf::construct!(super::UnsetNameInterpretation {
            shell_functions,
            shell_variables,
            name_references,
        });

        let names = bpaf::positional::<String>("NAME").many();

        bpaf::construct!(UnsetCommand {
            name_interpretation,
            names,
        })
    }

    fn about() -> &'static str {
        "Unset values and attributes of shell variables and functions."
    }

    fn synopsis() -> &'static str {
        "[-fnv] [NAME]..."
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
