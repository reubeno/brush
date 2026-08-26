//! `getopts` builtin: `GetOptsCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Parse command options.
#[derive(usage::Cli)]
#[usage(bin = "getopts", unknown_flags = "value", args_override_self = false)]
pub(crate) struct GetOptsCommand {
    /// Specification for options
    pub(super) options_string: String,

    /// Name of variable to receive next option
    pub(super) variable_name: String,

    /// Arguments to parse
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(GetOptsCommand);

impl FromArgs for GetOptsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for GetOptsCommand {
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
