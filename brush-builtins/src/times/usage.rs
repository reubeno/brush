//! `times` builtin: `TimesCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::{ExecutionResult, builtins, timing};
use brush_core::args::{ArgsError, FromArgs};

/// Report on usage time.
#[derive(usage::Cli)]
#[usage(bin = "times", unknown_flags = "error", args_override_self = false)]
pub(crate) struct TimesCommand {}

crate::impl_usage_parse!(TimesCommand);

impl FromArgs for TimesCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for TimesCommand {
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
