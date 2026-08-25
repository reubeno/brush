//! `builtin_` builtin: `BuiltinCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use brush_core::{ExecutionResult, };
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default)]
pub(crate) struct BuiltinCommand {
    pub(super) args: Vec<brush_core::CommandArg>,
}

impl crate::args::BpafArgs for BuiltinCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let args = bpaf::pure(Vec::new());
        bpaf::construct!(BuiltinCommand { args })
    }
fn about() -> &'static str {
        "Directly invokes a built-in, without going through typical search order."
    }
fn synopsis() -> &'static str {
        "SHELL_BUILTIN [ARGS]..."
    }
}

impl FromArgs for BuiltinCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for BuiltinCommand {
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
