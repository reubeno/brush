//! `alias` builtin: `AliasCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage aliases within the shell.
#[derive(Parser)]
pub(crate) struct AliasCommand {
    /// Print all defined aliases in a reusable format.
    #[arg(short = 'p')]
    pub(super) print: bool,

    /// List of aliases to display or update.
    #[arg(name = "name[=value]")]
    pub(super) aliases: Vec<String>,
}

impl builtins::Command for AliasCommand {
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
