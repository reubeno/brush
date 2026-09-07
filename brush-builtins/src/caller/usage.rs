//! `caller` builtin: `CallerCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};

/// Return the context of the current subroutine call.
#[derive(usage::Cli)]
#[usage(bin = "caller", unknown_flags = "error", args_override_self = false)]
pub(crate) struct CallerCommand {
    /// The number of call frames to go back.
    pub(super) expr: Option<usize>,
}

crate::impl_usage_parse!(CallerCommand);

impl FromArgs for CallerCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for CallerCommand {
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
