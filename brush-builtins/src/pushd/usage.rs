//! `pushd` builtin: `PushdCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Push a path onto the current directory stack.
#[derive(usage::Cli)]
#[usage(bin = "pushd", unknown_flags = "error", args_override_self = false)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[usage(short = 'n')]
    pub(super) no_directory_change: bool,

    /// Directory to push on the directory stack.
    pub(super) dir: String,
    //
    // TODO(pushd): implement +N and -N
}

crate::impl_usage_parse!(PushdCommand);

impl FromArgs for PushdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for PushdCommand {
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
