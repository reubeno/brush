//! `continue_` builtin: `ContinueCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Continue to the next iteration of a control-flow loop.
pub(crate) struct ContinueCommand {
    pub(super) which_loop: i8,
}

impl crate::args::BpafArgs for ContinueCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let which_loop = bpaf::positional::<i8>("WHICH_LOOP")
            .help("If specified, indicates which nested loop to continue to the next iteration of.")
            .fallback(1);
        bpaf::construct!(ContinueCommand { which_loop })
    }
fn about() -> &'static str {
        "Continue to the next iteration of a control-flow loop."
    }
fn synopsis() -> &'static str {
        "[N]"
    }
}

impl FromArgs for ContinueCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ContinueCommand {
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
