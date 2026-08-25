//! `hash` builtin: `HashCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use std::path::PathBuf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

#[derive(Bpaf)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[bpaf(short('d'))]
    pub(super) remove: bool,

    /// Display paths in a format usable for input.
    #[bpaf(short('l'))]
    pub(super) display_as_usable_input: bool,

    /// The path to associate with the names.
    #[bpaf(short('p'), argument("PATH"))]
    pub(super) path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[bpaf(short('r'))]
    pub(super) remove_all: bool,

    /// Display the paths associated with the names.
    #[bpaf(short('t'))]
    pub(super) display_paths: bool,

    /// Names to process.
    #[bpaf(positional("NAMES"))]
    pub(super) names: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for HashCommand {
fn parser() -> impl bpaf::Parser<Self> {
        hash_command()
    }
fn about() -> &'static str {
        "Remember or display program locations."
    }
fn synopsis() -> &'static str {
        "[-dlrt] [-p PATH] [NAMES]..."
    }
}

impl FromArgs for HashCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for HashCommand {
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
