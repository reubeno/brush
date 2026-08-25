//! `pwd` builtin: `PwdCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::{ExecutionResult, builtins};
use std::{borrow::Cow, io::Write, path::Path};
use brush_core::args::{ArgsError, FromArgs};

/// Display the current working directory.
#[derive(usage::Cli)]
#[usage(bin = "pwd", unknown_flags = "error", args_override_self = false)]
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    #[usage(short = 'P', overrides("-L"))]
    pub(super) physical: bool,

    /// Print $PWD if it names the current working directory.
    #[usage(short = 'L', overrides("-P"))]
    pub(super) allow_symlinks: bool,
}

crate::impl_usage_parse!(PwdCommand);

impl FromArgs for PwdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for PwdCommand {
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
