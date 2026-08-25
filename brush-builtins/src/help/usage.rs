//! `help` builtin: `HelpCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::{ExecutionResult, builtins};
use itertools::Itertools;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};

/// Display command help.
#[derive(usage::Cli)]
#[usage(bin = "help", unknown_flags = "error", args_override_self = false)]
pub(crate) struct HelpCommand {
    /// Display a short description for the commands.
    #[usage(short = 'd')]
    pub(super) short_description: bool,

    /// Display a man-style page of documentation for the commands.
    #[usage(short = 'm')]
    pub(super) man_page_style: bool,

    /// Display a short usage summary for the commands.
    #[usage(short = 's')]
    pub(super) short_usage: bool,

    /// Patterns of topics to display help for.
    pub(super) topic_patterns: Vec<String>,
}

crate::impl_usage_parse!(HelpCommand);

impl FromArgs for HelpCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for HelpCommand {
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
