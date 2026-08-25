//! `let_` builtin: `LetCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::{ExecutionExitCode, ExecutionResult, arithmetic::Evaluatable, builtins};
use brush_core::args::{ArgsError, FromArgs};

/// Evaluate arithmetic expressions.
#[derive(usage::Cli)]
#[usage(bin = "let", unknown_flags = "value", args_override_self = false)]
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) exprs: Vec<String>,
}

crate::impl_usage_parse!(LetCommand);

impl FromArgs for LetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for LetCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
