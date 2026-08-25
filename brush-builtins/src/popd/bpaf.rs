//! `popd` builtin: `PopdCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Pop a path from the current directory stack.
#[derive(Bpaf)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[bpaf(short('n'))]
    pub(super) no_directory_change: bool,
    //
    // TODO(popd): implement +N and -N
}

impl crate::args::BpafArgs for PopdCommand {
fn parser() -> impl bpaf::Parser<Self> {
        popd_command()
    }
fn about() -> &'static str {
        "Pop a path from the current directory stack."
    }
fn synopsis() -> &'static str {
        "[-n]"
    }
}

impl FromArgs for PopdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for PopdCommand {
    type Error = crate::dirs::DirError;

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
