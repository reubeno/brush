//! `umask` builtin: `UmaskCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use brush_core::builtins;
use clap::Parser;

/// Manage the process umask.
#[derive(Parser)]
pub(crate) struct UmaskCommand {
    /// If MODE is omitted, output in a form that may be reused as input.
    #[arg(short = 'p')]
    pub(super) print_roundtrippable: bool,

    /// Makes the output symbolic; otherwise an octal number is given.
    #[arg(short = 'S')]
    pub(super) symbolic_output: bool,

    /// Mode mask.
    pub(super) mode: Option<String>,
}

impl builtins::Command for UmaskCommand {
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
