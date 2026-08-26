//! `kill` builtin: `KillCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Signal a job or process.
#[derive(usage::Cli)]
#[usage(bin = "kill", unknown_flags = "value", args_override_self = false)]
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    #[usage(short = 's', value_name = "SIG_NAME")]
    pub(super) signal_name: Option<String>,

    /// Number of the signal to send.
    #[usage(short = 'n', value_name = "SIG_NUM")]
    pub(super) signal_number: Option<usize>,

    //
    // TODO(kill): implement -sigspec syntax
    /// List known signal names.
    #[usage(short = 'l', short = 'L')]
    pub(super) list_signals: bool,

    // Interpretation of these depends on whether -l is present.
    #[usage(allow_negative_numbers)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(KillCommand);

impl FromArgs for KillCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for KillCommand {
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
