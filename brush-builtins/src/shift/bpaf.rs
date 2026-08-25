//! `shift` builtin: `ShiftCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Shift positional arguments.
pub(crate) struct ShiftCommand {
    pub(super) n: Option<i32>,
}

impl crate::args::BpafArgs for ShiftCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let n = bpaf::positional::<i32>("N")
            .help("Number of positions to shift the arguments by (defaults to 1).")
            .optional();
        bpaf::construct!(ShiftCommand { n })
    }
fn about() -> &'static str {
        "Shift positional arguments."
    }
fn synopsis() -> &'static str {
        "[N]"
    }
}

impl FromArgs for ShiftCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ShiftCommand {
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
