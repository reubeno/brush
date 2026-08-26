//! `return_` builtin: `ReturnCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Return from the current function.
#[derive(Bpaf)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    #[bpaf(positional("CODE"))]
    pub(super) code: Option<i32>,
}

impl crate::args::bpaf_support::BpafArgs for ReturnCommand {
fn parser() -> impl bpaf::Parser<Self> {
        return_command()
    }
fn about() -> &'static str {
        "Return from the current function."
    }
fn synopsis() -> &'static str {
        "[CODE]"
    }
}

impl FromArgs for ReturnCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ReturnCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
