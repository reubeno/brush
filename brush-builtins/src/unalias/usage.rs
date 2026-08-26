//! `unalias` builtin: `UnaliasCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Unset a shell alias.
#[derive(usage::Cli)]
#[usage(bin = "unalias", unknown_flags = "error", args_override_self = false)]
pub(crate) struct UnaliasCommand {
    /// Remove all aliases.
    #[usage(short = 'a')]
    pub(super) remove_all: bool,

    /// Names of aliases to operate on.
    pub(super) aliases: Vec<String>,
}

crate::impl_usage_parse!(UnaliasCommand);

impl FromArgs for UnaliasCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for UnaliasCommand {
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
