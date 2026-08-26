//! `exit` builtin: `ExitCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Exit the shell.
#[derive(usage::Cli)]
#[usage(bin = "exit", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional;
    // `allow_negative_numbers` covers the `-1`-style codes this builtin needs.
    #[usage(allow_negative_numbers)]
    pub(super) code: Option<i64>,
}

crate::impl_usage_parse!(ExitCommand);

impl FromArgs for ExitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ExitCommand {
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
