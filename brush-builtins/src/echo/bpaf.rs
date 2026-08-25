//! `echo` builtin: `EchoCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Echo text to standard output.
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    pub(super) no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    pub(super) interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    pub(super) no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    pub(super) args: Vec<String>,
}

impl crate::args::BpafArgs for EchoCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let no_trailing_newline = bpaf::short('n')
            .help("Suppress the trailing newline from the output.")
            .switch();
        let interpret_backslash_escapes = bpaf::short('e')
            .help("Interpret backslash escapes in the provided text.")
            .switch();
        let no_interpret_backslash_escapes = bpaf::short('E')
            .help("Do not interpret backslash escapes in the provided text.")
            .switch();
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(EchoCommand {
            no_trailing_newline,
            interpret_backslash_escapes,
            no_interpret_backslash_escapes,
            args,
        })
    }
fn about() -> &'static str {
        "Echo text to standard output."
    }
fn synopsis() -> &'static str {
        "[-neE] [TOKENS]..."
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }
}

impl FromArgs for EchoCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for EchoCommand {
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
