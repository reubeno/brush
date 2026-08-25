//! `dot` builtin: `DotCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::path::Path;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Evaluate the provided script in the current shell environment.
#[derive(usage::Cli)]
#[usage(bin = "dot", unknown_flags = "value", args_override_self = false)]
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    pub(super) script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) script_args: Vec<String>,
}

crate::impl_usage_parse!(DotCommand);

impl FromArgs for DotCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for DotCommand {
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
