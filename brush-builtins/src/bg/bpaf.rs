//! `bg` builtin: `BgCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Moves a job to run in the background.
#[derive(Bpaf)]
pub(crate) struct BgCommand {
    /// List of job specs to move to background.
    #[bpaf(positional("JOB_SPECS"))]
    pub(super) job_specs: Vec<String>,
}

impl crate::args::BpafArgs for BgCommand {
fn parser() -> impl bpaf::Parser<Self> {
        bg_command()
    }
fn about() -> &'static str {
        "Moves a job to run in the background."
    }
fn synopsis() -> &'static str {
        "[JOB_SPECS]..."
    }
}

impl FromArgs for BgCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for BgCommand {
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
