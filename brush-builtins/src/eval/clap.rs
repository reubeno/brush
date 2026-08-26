//! `eval` builtin: `EvalCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use brush_core::builtins;
use clap::Parser;

/// Evaluate the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[clap(allow_hyphen_values = true)]
    pub(super) args: Vec<String>,
}

impl builtins::Command for EvalCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
