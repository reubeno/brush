//! `command` builtin: `CommandCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use std::{fmt::Display, io::Write, path::Path};
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

enum FoundCommand {
    Builtin(String),
    External(String),
}

/// Directly invokes an external command, without going through typical search order.
#[derive(Default)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    pub(super) use_default_path: bool,

    /// Display a short description of the command.
    pub(super) print_description: bool,

    /// Display a more verbose description of the command.
    pub(super) print_verbose_description: bool,

    /// Command and arguments.
    pub(super) command_and_args: Vec<String>,
}

impl crate::args::BpafArgs for CommandCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let use_default_path = bpaf::short('p').help("Use default PATH value.").switch();
        let print_description = bpaf::short('v')
            .help("Display a short description of the command.")
            .switch();
        let print_verbose_description = bpaf::short('V')
            .help("Display a more verbose description of the command.")
            .switch();
        let command_and_args = bpaf::pure(Vec::new());

        bpaf::construct!(CommandCommand {
            use_default_path,
            print_description,
            print_verbose_description,
            command_and_args,
        })
    }
fn about() -> &'static str {
        "Directly invokes an external command, without going through typical search order."
    }
fn synopsis() -> &'static str {
        "[-pvV] [COMMAND [ARG]...]"
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.command_and_args = args;
    }
}

impl FromArgs for CommandCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for CommandCommand {
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
