//! `popd` builtin: `PopdCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Pop a path from the current directory stack.
#[derive(usage::Cli)]
#[usage(bin = "popd", unknown_flags = "error", args_override_self = false)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[usage(short = 'n')]
    pub(super) no_directory_change: bool,
    //
    // TODO(popd): implement +N and -N
}

crate::impl_usage_parse!(PopdCommand);

impl FromArgs for PopdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for PopdCommand {
    type Error = crate::dirs::DirError;

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
