//! `eval` builtin: `EvalCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use brush_core::{ExecutionResult, };
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Evaluate the given string as script.
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    pub(super) args: Vec<String>,
}

impl crate::args::BpafArgs for EvalCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(EvalCommand { args })
    }
fn about() -> &'static str {
        "Evaluate the given string as script."
    }
fn synopsis() -> &'static str {
        "[COMMAND]..."
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }
}

impl FromArgs for EvalCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for EvalCommand {
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
