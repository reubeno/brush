//! `suspend` builtin: `SuspendCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Suspend the shell.
#[derive(usage::Cli)]
#[usage(bin = "suspend", unknown_flags = "error", args_override_self = false)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[usage(short = 'f')]
    pub(super) force: bool,
}

crate::impl_usage_parse!(SuspendCommand);

impl FromArgs for SuspendCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for SuspendCommand {
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
