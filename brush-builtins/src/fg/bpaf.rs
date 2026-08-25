//! `fg` builtin: `FgCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Move a specified job to the foreground.
#[derive(Bpaf)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    #[bpaf(positional("JOB_SPEC"))]
    pub(super) job_spec: Option<String>,
}

impl crate::args::BpafArgs for FgCommand {
fn parser() -> impl bpaf::Parser<Self> {
        fg_command()
    }
fn about() -> &'static str {
        "Move a specified job to the foreground."
    }
fn synopsis() -> &'static str {
        "[JOB_SPEC]"
    }
}

impl FromArgs for FgCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for FgCommand {
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
