//! `suspend` builtin: `SuspendCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Bpaf;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Suspend the shell.
#[derive(Bpaf)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[bpaf(short('f'))]
    pub(super) force: bool,
}

impl crate::args::BpafArgs for SuspendCommand {
fn parser() -> impl bpaf::Parser<Self> {
        suspend_command()
    }
fn about() -> &'static str {
        "Suspend the shell."
    }
fn synopsis() -> &'static str {
        "[-f]"
    }
}

impl FromArgs for SuspendCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for SuspendCommand {
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
