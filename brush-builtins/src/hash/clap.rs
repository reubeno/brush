//! `hash` builtin: `HashCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use std::path::PathBuf;
use brush_core::builtins;

#[derive(Parser)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[arg(short = 'd')]
    pub(super) remove: bool,

    /// Display paths in a format usable for input.
    #[arg(short = 'l')]
    pub(super) display_as_usable_input: bool,

    /// The path to associate with the names.
    #[arg(short = 'p', value_name = "PATH")]
    pub(super) path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[arg(short = 'r')]
    pub(super) remove_all: bool,

    /// Display the paths associated with the names.
    #[arg(short = 't')]
    pub(super) display_paths: bool,

    /// Names to process.
    pub(super) names: Vec<String>,
}

impl builtins::Command for HashCommand {
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
