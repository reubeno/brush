//! `exit` builtin: `ExitCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Exit the shell.
pub(crate) struct ExitCommand {
    /// The exit code to return.
    pub(super) code: Option<i64>,
}

impl crate::args::BpafArgs for ExitCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let code = bpaf::pure(None);

        bpaf::construct!(ExitCommand { code })
    }
fn about() -> &'static str {
        "Exit the shell."
    }
fn synopsis() -> &'static str {
        "[N]"
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, mut args: Vec<String>) {
        self.code = if args.is_empty() {
            None
        } else {
            Some(args.remove(0))
        };
    }

    fn set_trailing_args(&mut self, mut args: Vec<String>) {
        self.code = if args.is_empty() {
            None
        } else {
            let first = args.remove(0);
            Some(first.parse::<i64>().unwrap_or(0))
        };
    }

}

impl FromArgs for ExitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ExitCommand {
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
