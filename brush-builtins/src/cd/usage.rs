//! `cd` builtin: `CdCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use std::path::PathBuf;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Change the current shell working directory.
#[derive(usage::Cli)]
#[usage(bin = "cd", unknown_flags = "error", args_override_self = false)]
pub(crate) struct CdCommand {
    /// Force following symlinks.
    #[usage(short = 'L', overrides("-P"))]
    pub(super) force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    #[usage(short = 'P', overrides("-L"))]
    pub(super) use_physical_dir: bool,

    /// Exit with non zero exit status if current working directory resolution fails.
    #[usage(short = 'e')]
    pub(super) exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended
    /// attributes.
    #[usage(short = '@')]
    pub(super) file_with_xattr_as_dir: bool,

    /// By default it is the value of the HOME shell variable. If `TARGET_DIR` is "-", it is
    /// converted to $OLDPWD.
    pub(super) target_dir: Option<PathBuf>,
}

crate::impl_usage_parse!(CdCommand);

impl FromArgs for CdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for CdCommand {
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
