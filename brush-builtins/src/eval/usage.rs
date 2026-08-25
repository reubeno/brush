//! `eval` builtin: `EvalCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::{ExecutionResult, builtins};
use brush_core::args::{ArgsError, FromArgs};

/// Evaluate the given string as script.
#[derive(usage::Cli)]
#[usage(bin = "eval", unknown_flags = "value", args_override_self = false)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional; it is
    // normalized into the `trailing_var_arg` boundary (`double_dash = "automatic"`).
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(EvalCommand);

impl FromArgs for EvalCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for EvalCommand {
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
