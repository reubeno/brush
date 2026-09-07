//! `dot` builtin: `DotCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Evaluate the provided script in the current shell environment.
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    pub(super) script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    pub(super) script_args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for DotCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let script_path = bpaf::pure(String::new());
        let script_args = bpaf::pure(Vec::new());

        bpaf::construct!(DotCommand {
            script_path,
            script_args,
        })
    }
fn about() -> &'static str {
        "Evaluate the provided script in the current shell environment."
    }
fn synopsis() -> &'static str {
        "SCRIPT_PATH [ARGS]..."
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        let mut iter = args.into_iter();
        if let Some(script_path) = iter.next() {
            self.script_path = script_path;
        }
        self.script_args = iter.collect();
    }
}

impl FromArgs for DotCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for DotCommand {
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
