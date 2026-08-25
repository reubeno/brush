//! `complete` builtin: `CompleteCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use brush_core::completion::{self, CompleteAction, CompleteOption, Spec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};
use brush_core::args::{ArgsError, FromArgs};

/// Configure programmable command completion.
#[derive(usage::Cli)]
#[usage(bin = "complete", unknown_flags = "error", args_override_self = false)]
pub(crate) struct CompleteCommand {
    /// Display registered completion settings.
    #[usage(short = 'p')]
    pub(super) print: bool,

    /// Remove the completion settings associated with the given command.
    #[usage(short = 'r')]
    pub(super) remove: bool,

    /// Apply these settings to the default completion scenario.
    #[usage(short = 'D')]
    pub(super) use_as_default: bool,

    /// Apply these settings to completion of empty lines.
    #[usage(short = 'E')]
    pub(super) use_for_empty_line: bool,

    /// Apply these settings to completion of the initial word of the input line.
    #[usage(short = 'I')]
    pub(super) use_for_initial_word: bool,

    #[usage(flatten)]
    pub(super) common_args: CommonCompleteCommandArgs,

    pub(super) names: Vec<String>,
}

crate::impl_usage_parse!(CompleteCommand);

impl FromArgs for CompleteCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for CompleteCommand {
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
