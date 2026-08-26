//! `dirs` builtin: `DirsCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Manage the current directory stack.
#[derive(Default, usage::Cli)]
#[usage(bin = "dirs", unknown_flags = "error", args_override_self = false)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[usage(short = 'c')]
    pub(super) clear: bool,

    /// Don't tilde-shorten paths.
    #[usage(short = 'l')]
    pub(super) tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[usage(short = 'p')]
    pub(super) print_one_per_line: bool,

    /// Print one directory per line with its index.
    #[usage(short = 'v')]
    pub(super) print_one_per_line_with_index: bool,
    //
    // TODO(dirs): implement +N and -N
}

crate::impl_usage_parse!(DirsCommand);

impl FromArgs for DirsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for DirsCommand {
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
