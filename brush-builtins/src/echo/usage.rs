//! `echo` builtin: `EchoCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Echo text to standard output.
#[derive(usage::Cli)]
#[usage(
    bin = "echo",
    unknown_flags = "value",
    args_override_self = false,
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[usage(short = 'n')]
    pub(super) no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[usage(short = 'e')]
    pub(super) interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[usage(short = 'E')]
    pub(super) no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(EchoCommand);

impl FromArgs for EchoCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for EchoCommand {
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
