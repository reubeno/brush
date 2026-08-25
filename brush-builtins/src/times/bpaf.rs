//! `times` builtin: `TimesCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Report on usage time.
#[derive(Clone)]
pub(crate) struct TimesCommand {}

impl crate::args::BpafArgs for TimesCommand {
fn parser() -> impl bpaf::Parser<Self> {
        bpaf::construct!(TimesCommand {})
    }
fn about() -> &'static str {
        "Report on usage time."
    }
fn synopsis() -> &'static str {
        ""
    }
}

impl FromArgs for TimesCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for TimesCommand {
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
