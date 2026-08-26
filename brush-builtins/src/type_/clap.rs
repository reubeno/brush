//! `type_` builtin: `TypeCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Inspect the type of a named shell item.
#[derive(Parser)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[arg(short = 'a')]
    pub(super) all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[arg(short = 'f')]
    pub(super) suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[arg(short = 'P')]
    pub(super) force_path_search: bool,

    /// Show file path only.
    #[arg(short = 'p')]
    pub(super) show_path_only: bool,

    /// Only display the type of the specified name.
    #[arg(short = 't')]
    pub(super) type_only: bool,

    /// Names to search for.
    pub(super) names: Vec<String>,
}

impl builtins::Command for TypeCommand {
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
