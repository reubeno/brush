//! `caller` builtin: `CallerCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Return the context of the current subroutine call.
#[derive(Bpaf)]
pub(crate) struct CallerCommand {
    /// The number of call frames to go back.
    #[bpaf(positional("EXPR"))]
    pub(super) expr: Option<usize>,
}

impl crate::args::BpafArgs for CallerCommand {
fn parser() -> impl bpaf::Parser<Self> {
        caller_command()
    }
fn about() -> &'static str {
        "Return the context of the current subroutine call."
    }
fn synopsis() -> &'static str {
        "[EXPR]"
    }
}

impl FromArgs for CallerCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for CallerCommand {
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
