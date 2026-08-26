//! `break_` builtin: `BreakCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Breaks out of a control-flow loop.
#[derive(usage::Cli)]
#[usage(bin = "break", unknown_flags = "error", args_override_self = false)]
pub(crate) struct BreakCommand {
    /// If specified, indicates which nested loop to break out of.
    #[usage(default = "1")]
    pub(super) which_loop: i8,
}

crate::impl_usage_parse!(BreakCommand);

impl FromArgs for BreakCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for BreakCommand {
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
