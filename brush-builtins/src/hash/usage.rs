//! `hash` builtin: `HashCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::{io::Write, path::PathBuf};
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

#[derive(usage::Cli)]
#[usage(bin = "hash", unknown_flags = "error", args_override_self = false)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[usage(short = 'd')]
    pub(super) remove: bool,

    /// Display paths in a format usable for input.
    #[usage(short = 'l')]
    pub(super) display_as_usable_input: bool,

    /// The path to associate with the names.
    #[usage(short = 'p', value_name = "PATH")]
    pub(super) path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[usage(short = 'r')]
    pub(super) remove_all: bool,

    /// Display the paths associated with the names.
    #[usage(short = 't')]
    pub(super) display_paths: bool,

    /// Names to process.
    pub(super) names: Vec<String>,
}

crate::impl_usage_parse!(HashCommand);

impl FromArgs for HashCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for HashCommand {
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
