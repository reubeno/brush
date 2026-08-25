//! `shopt` builtin: `ShoptCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage shopt-style options.
#[derive(Parser)]
pub(crate) struct ShoptCommand {
    /// Manage set -o options.
    #[arg(short = 'o')]
    pub(super) set_o_names_only: bool,

    /// Print options' current values.
    #[arg(short = 'p')]
    pub(super) print: bool,

    /// Suppress typical output.
    #[arg(short = 'q')]
    pub(super) quiet: bool,

    /// Set the specified options.
    #[arg(short = 's')]
    pub(super) set: bool,

    /// Unset the specified options.
    #[arg(short = 'u')]
    pub(super) unset: bool,

    /// Names of options to operate on.
    pub(super) options: Vec<String>,
}

impl builtins::Command for ShoptCommand {
    type Error = brush_core::Error;

    #[allow(clippy::too_many_lines)]
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
