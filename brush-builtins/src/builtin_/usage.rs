//! `builtin_` builtin: `BuiltinCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::{ExecutionResult, builtins};
use brush_core::args::{ArgsError, FromArgs};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default, usage::Cli)]
#[usage(bin = "builtin", unknown_flags = "error", args_override_self = false)]
pub(crate) struct BuiltinCommand {
    #[usage(skip)]
    pub(super) args: Vec<brush_core::CommandArg>,
}

crate::impl_usage_parse!(BuiltinCommand);

impl FromArgs for BuiltinCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for BuiltinCommand {
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

impl builtins::DeclarationCommand for BuiltinCommand {
    fn set_declarations(&mut self, args: Vec<brush_core::CommandArg>) {
        self.args = args;
    }
}
