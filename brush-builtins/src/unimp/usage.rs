//! `unimp` builtin: `UnimplementedCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// (UNIMPLEMENTED COMMAND)
#[derive(usage::Cli)]
#[usage(bin = "unimp", unknown_flags = "value", args_override_self = false)]
pub(crate) struct UnimplementedCommand {
    #[usage(arg, double_dash = "automatic")]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(UnimplementedCommand);

impl FromArgs for UnimplementedCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for UnimplementedCommand {
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
