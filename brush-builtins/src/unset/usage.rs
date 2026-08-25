//! `unset` builtin: `UnsetCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::borrow::Cow;
use std::io::Write;
use brush_core::{ExecutionExitCode, ExecutionResult, Shell, builtins};
use brush_core::args::{ArgsError, FromArgs};

/// Unset a variable.
#[derive(usage::Cli)]
#[usage(bin = "unset", unknown_flags = "error", args_override_self = false)]
pub(crate) struct UnsetCommand {
    #[usage(flatten)]
    pub(super) name_interpretation: UnsetNameInterpretation,

    /// Names of variables to unset.
    pub(super) names: Vec<String>,
}

crate::impl_usage_parse!(UnsetCommand);

impl FromArgs for UnsetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for UnsetCommand {
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
