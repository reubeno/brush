//! `type_` builtin: `TypeCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use std::path::PathBuf;
use brush_parser::ast;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

enum ResolvedType<'a> {
    Alias(String),
    Keyword,
    Function(&'a ast::FunctionDefinition),
    Builtin,
    File { path: PathBuf, hashed: bool },
}

/// Inspect the type of a named shell item.
#[derive(Bpaf)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[bpaf(short('a'))]
    pub(super) all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[bpaf(short('f'))]
    pub(super) suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[bpaf(short('P'))]
    pub(super) force_path_search: bool,

    /// Show file path only.
    #[bpaf(short('p'))]
    pub(super) show_path_only: bool,

    /// Only display the type of the specified name.
    #[bpaf(short('t'))]
    pub(super) type_only: bool,

    /// Names to search for.
    #[bpaf(positional("NAMES"))]
    pub(super) names: Vec<String>,
}

impl crate::args::BpafArgs for TypeCommand {
fn parser() -> impl bpaf::Parser<Self> {
        type_command()
    }
fn about() -> &'static str {
        "Inspect the type of a named shell item."
    }
fn synopsis() -> &'static str {
        "[-aPptf] [NAMES]..."
    }
}

impl FromArgs for TypeCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for TypeCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
