//! `shopt` builtin: `ShoptCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools;
use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Manage shopt-style options.
#[derive(usage::Cli)]
#[usage(bin = "shopt", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ShoptCommand {
    /// Manage set -o options.
    #[usage(short = 'o')]
    pub(super) set_o_names_only: bool,

    /// Print options' current values.
    #[usage(short = 'p')]
    pub(super) print: bool,

    /// Suppress typical output.
    #[usage(short = 'q')]
    pub(super) quiet: bool,

    /// Set the specified options.
    #[usage(short = 's')]
    pub(super) set: bool,

    /// Unset the specified options.
    #[usage(short = 'u')]
    pub(super) unset: bool,

    /// Names of options to operate on.
    pub(super) options: Vec<String>,
}

crate::impl_usage_parse!(ShoptCommand);

impl FromArgs for ShoptCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ShoptCommand {
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
