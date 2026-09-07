//! `let_` builtin: `LetCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Evaluate arithmetic expressions.
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    pub(super) exprs: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for LetCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let exprs = bpaf::pure(Vec::new());

        bpaf::construct!(LetCommand { exprs })
    }
fn about() -> &'static str {
        "Evaluate arithmetic expressions."
    }
fn synopsis() -> &'static str {
        "[EXPRESSION]..."
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.exprs = args;
    }
}

impl FromArgs for LetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for LetCommand {
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
