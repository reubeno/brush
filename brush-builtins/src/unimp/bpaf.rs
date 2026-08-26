//! `unimp` builtin: `UnimplementedCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// (UNIMPLEMENTED COMMAND)
pub(crate) struct UnimplementedCommand {
    pub(super) args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for UnimplementedCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // Capture all arguments verbatim; no option parsing is performed.
        let args = bpaf::any("ARGS", Some).many();
        bpaf::construct!(UnimplementedCommand { args })
    }
}

impl FromArgs for UnimplementedCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for UnimplementedCommand {
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
