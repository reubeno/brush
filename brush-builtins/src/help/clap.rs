//! `help` builtin: `HelpCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use brush_core::builtins;
use clap::Parser;

/// Display command help.
#[derive(Parser)]
pub(crate) struct HelpCommand {
    /// Display a short description for the commands.
    #[arg(short = 'd')]
    pub(super) short_description: bool,

    /// Display a man-style page of documentation for the commands.
    #[arg(short = 'm')]
    pub(super) man_page_style: bool,

    /// Display a short usage summary for the commands.
    #[arg(short = 's')]
    pub(super) short_usage: bool,

    /// Patterns of topics to display help for.
    pub(super) topic_patterns: Vec<String>,
}

impl builtins::Command for HelpCommand {
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
