//! `break_` builtin: `BreakCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Breaks out of a control-flow loop.
pub(crate) struct BreakCommand {
    pub(super) which_loop: i8,
}

impl crate::args::bpaf_support::BpafArgs for BreakCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let which_loop = bpaf::positional::<i8>("WHICH_LOOP")
            .help("If specified, indicates which nested loop to break out of.")
            .fallback(1);
        bpaf::construct!(BreakCommand { which_loop })
    }
fn about() -> &'static str {
        "Breaks out of a control-flow loop."
    }
fn synopsis() -> &'static str {
        "[N]"
    }
}

impl FromArgs for BreakCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for BreakCommand {
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
