//! `dirs` builtin: `DirsCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DirError {
    /// Directory stack is empty.
    #[error("directory stack is empty")]
    DirStackEmpty,

    /// A shell error occurred.
    #[error(transparent)]
    ShellError(#[from] brush_core::Error),
}

/// Manage the current directory stack.
#[derive(Default, Bpaf)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[bpaf(short('c'))]
    pub(super) clear: bool,

    /// Don't tilde-shorten paths.
    #[bpaf(short('l'))]
    pub(super) tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[bpaf(short('p'))]
    pub(super) print_one_per_line: bool,

    /// Print one directory per line with its index.
    #[bpaf(short('v'))]
    pub(super) print_one_per_line_with_index: bool,
    //
    // TODO(dirs): implement +N and -N
}

impl crate::args::bpaf_support::BpafArgs for DirsCommand {
fn parser() -> impl bpaf::Parser<Self> {
        dirs_command()
    }
fn about() -> &'static str {
        "Manage the current directory stack."
    }
fn synopsis() -> &'static str {
        "[-clpv]"
    }
}

impl FromArgs for DirsCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for DirsCommand {
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
