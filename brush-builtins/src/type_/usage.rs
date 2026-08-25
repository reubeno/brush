//! `type_` builtin: `TypeCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use std::path::{Path, PathBuf};
use brush_core::sys::{self, fs::PathExt};
use brush_core::{ExecutionResult, Shell, builtins, parser::ast};
use brush_core::args::{ArgsError, FromArgs};

/// Inspect the type of a named shell item.
#[derive(usage::Cli)]
#[usage(bin = "type", unknown_flags = "error", args_override_self = false)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[usage(short = 'a')]
    pub(super) all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[usage(short = 'f')]
    pub(super) suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[usage(short = 'P')]
    pub(super) force_path_search: bool,

    /// Show file path only.
    #[usage(short = 'p')]
    pub(super) show_path_only: bool,

    /// Only display the type of the specified name.
    #[usage(short = 't')]
    pub(super) type_only: bool,

    /// Names to search for.
    pub(super) names: Vec<String>,
}

crate::impl_usage_parse!(TypeCommand);

impl FromArgs for TypeCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for TypeCommand {
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
