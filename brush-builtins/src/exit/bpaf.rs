//! `exit` builtin: `ExitCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Exit the shell.
pub(crate) struct ExitCommand {
    /// The exit code to return.
    pub(super) code: Option<i64>,
}

impl crate::args::bpaf_support::BpafArgs for ExitCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let code = bpaf::pure(None);

        bpaf::construct!(ExitCommand { code })
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn set_trailing_args(&mut self, mut args: Vec<String>) {
        self.code = if args.is_empty() {
            None
        } else {
            Some(args.remove(0).parse::<i64>().unwrap_or(0))
        };
    }
}

impl FromArgs for ExitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ExitCommand {
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
