//! `pushd` builtin: `PushdCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Push a path onto the current directory stack.
#[derive(Bpaf)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[bpaf(short('n'))]
    pub(super) no_directory_change: bool,

    /// Directory to push on the directory stack.
    #[bpaf(positional("DIR"))]
    pub(super) dir: String,
    //
    // TODO(pushd): implement +N and -N
}

impl crate::args::BpafArgs for PushdCommand {
fn parser() -> impl bpaf::Parser<Self> {
        pushd_command()
    }
fn about() -> &'static str {
        "Push a path onto the current directory stack."
    }
fn synopsis() -> &'static str {
        "[-n] [DIR]"
    }
}

impl FromArgs for PushdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for PushdCommand {
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
