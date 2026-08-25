//! `enable` builtin: `EnableCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::ExecutionResult;
use itertools::Itertools;
use std::io::Write;
use brush_core::builtins;
use brush_core::error;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Enable, disable, or display built-in commands.
#[derive(usage::Cli)]
#[usage(bin = "enable", unknown_flags = "error", args_override_self = false)]
pub(crate) struct EnableCommand {
    /// Print a list of built-in commands.
    #[usage(short = 'a')]
    pub(super) print_list: bool,

    /// Disables the specified built-in commands.
    #[usage(short = 'n')]
    pub(super) disable: bool,

    /// Print a list of built-in commands with reusable output.
    #[usage(short = 'p')]
    pub(super) print_reusably: bool,

    /// Only operate on special built-in commands.
    #[usage(short = 's')]
    pub(super) special_only: bool,

    /// Path to a shared object from which built-in commands will be loaded.
    #[usage(short = 'f', value_name = "PATH")]
    pub(super) shared_object_path: Option<String>,

    /// Remove the built-in commands loaded from the indicated object path.
    #[usage(short = 'd')]
    pub(super) remove_loaded_builtin: bool,

    /// Names of built-in commands to operate on.
    pub(super) names: Vec<String>,
}

crate::impl_usage_parse!(EnableCommand);

impl FromArgs for EnableCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for EnableCommand {
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
