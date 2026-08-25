//! `unalias` builtin: `UnaliasCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Bpaf;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Unset a shell alias.
#[derive(Bpaf)]
pub(crate) struct UnaliasCommand {
    /// Remove all aliases.
    #[bpaf(short('a'))]
    pub(super) remove_all: bool,

    /// Names of aliases to operate on.
    #[bpaf(positional("ALIASES"))]
    pub(super) aliases: Vec<String>,
}

impl crate::args::BpafArgs for UnaliasCommand {
fn parser() -> impl bpaf::Parser<Self> {
        unalias_command()
    }
fn about() -> &'static str {
        "Unset a shell alias."
    }
fn synopsis() -> &'static str {
        "[-a] [ALIASES]..."
    }
}

impl FromArgs for UnaliasCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for UnaliasCommand {
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
