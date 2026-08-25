//! `bg` builtin: `BgCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Moves a job to run in the background.
#[derive(Parser)]
pub(crate) struct BgCommand {
    /// List of job specs to move to background.
    pub(super) job_specs: Vec<String>,
}

impl builtins::Command for BgCommand {
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
