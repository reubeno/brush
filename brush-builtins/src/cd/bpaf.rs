//! `cd` builtin: `CdCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]


use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

use std::path::PathBuf;

/// Change the current shell working directory.
pub(crate) struct CdCommand {
    /// Force following symlinks.
    pub(super) force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    pub(super) use_physical_dir: bool,

    /// Exit with non zero exit status if current working directory resolution fails.
    pub(super) exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended attributes.
    pub(super) file_with_xattr_as_dir: bool,

    /// By default it is the value of the HOME shell variable. If `TARGET_DIR` is "-", it is
    /// converted to $OLDPWD.
    pub(super) target_dir: Option<PathBuf>,
}

impl crate::args::bpaf_support::BpafArgs for CdCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        let exit_on_failed_cwd_resolution = bpaf::short('e')
            .help("Exit with non zero exit status if current working directory resolution fails.")
            .switch();
        let file_with_xattr_as_dir = bpaf::short('@')
            .help("Show file with extended attributes as a dir with extended attributes.")
            .switch();

        let force_follow_symlinks = bpaf::short('L')
            .help("Force following symlinks.")
            .switch();
        let use_physical_dir = bpaf::short('P')
            .help("Use physical dir structure without following symlinks.")
            .switch();

        let target_dir = bpaf::positional::<PathBuf>("TARGET_DIR")
            .help(
                "By default it is the value of the HOME shell variable. If `TARGET_DIR` is \"-\", \
                it is converted to $OLDPWD.",
            )
            .optional();

        bpaf::construct!(CdCommand {
            force_follow_symlinks,
            use_physical_dir,
            exit_on_failed_cwd_resolution,
            file_with_xattr_as_dir,
            target_dir,
        })
    }

    fn about() -> &'static str {
        "Change the current shell working directory."
    }

    fn synopsis() -> &'static str {
        "[-LPe@] [TARGET_DIR]"
    }
}

impl FromArgs for CdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for CdCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
