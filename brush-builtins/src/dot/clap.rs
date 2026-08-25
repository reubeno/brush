//! `dot` builtin: `DotCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use brush_core::builtins;
use clap::Parser;

/// Evaluate the provided script in the current shell environment.
#[derive(Parser)]
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    pub(super) script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(super) script_args: Vec<String>,
}

impl builtins::Command for DotCommand {
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
