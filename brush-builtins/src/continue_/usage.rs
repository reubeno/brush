//! `continue_` builtin: `ContinueCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};
use brush_core::args::{ArgsError, FromArgs};

/// Continue to the next iteration of a control-flow loop.
#[derive(usage::Cli)]
#[usage(bin = "continue", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[usage(default = "1")]
    pub(super) which_loop: i8,
}

crate::impl_usage_parse!(ContinueCommand);

impl FromArgs for ContinueCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ContinueCommand {
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
