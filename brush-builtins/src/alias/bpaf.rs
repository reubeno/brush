//! `alias` builtin: `AliasCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Bpaf;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Manage aliases within the shell.
#[derive(Bpaf)]
pub(crate) struct AliasCommand {
    /// Print all defined aliases in a reusable format.
    #[bpaf(short('p'))]
    pub(super) print: bool,

    /// List of aliases to display or update.
    #[bpaf(positional("name[=value]"))]
    pub(super) aliases: Vec<String>,
}

impl crate::args::BpafArgs for AliasCommand {
fn parser() -> impl bpaf::Parser<Self> {
        alias_command()
    }
fn about() -> &'static str {
        "Manage aliases within the shell."
    }
fn synopsis() -> &'static str {
        "[-p] [name[=value]]..."
    }
}

impl FromArgs for AliasCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for AliasCommand {
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
