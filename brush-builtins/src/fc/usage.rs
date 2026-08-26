//! `fc` builtin: `FcCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};

/// Process command history list.
#[derive(usage::Cli)]
#[usage(bin = "fc", unknown_flags = "error", args_override_self = false)]
pub(crate) struct FcCommand {
    /// List commands instead of editing them.
    #[usage(short = 'l')]
    pub(super) list: bool,

    /// Suppress line numbers when listing.
    #[usage(short = 'n', requires("-l"))]
    pub(super) no_line_numbers: bool,

    /// Reverse the order of commands.
    #[usage(short = 'r')]
    pub(super) reverse: bool,

    /// Re-execute command after substitution (old=new format).
    #[usage(short = 's')]
    pub(super) substitute: bool,

    /// Editor to use (only relevant when not listing or substituting).
    #[usage(short = 'e', value_name = "ENAME")]
    pub(super) editor: Option<String>,

    /// First command in range (number or string prefix).
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional;
    // `allow_negative_numbers` covers `-N`-style offsets but not hyphen-leading prefixes.
    #[usage(value_name = "FIRST", allow_negative_numbers)]
    pub(super) first: Option<String>,

    /// Last command in range (number or string prefix).
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional;
    // `allow_negative_numbers` covers `-N`-style offsets but not hyphen-leading prefixes.
    #[usage(value_name = "LAST", allow_negative_numbers)]
    pub(super) last: Option<String>,
}

crate::impl_usage_parse!(FcCommand);

impl FromArgs for FcCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for FcCommand {
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
