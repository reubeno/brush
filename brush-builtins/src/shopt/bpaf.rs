//! `shopt` builtin: `ShoptCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Parser;
use itertools::Itertools;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Manage shopt-style options.
pub(crate) struct ShoptCommand {
    pub(super) set_o_names_only: bool,
    pub(super) print: bool,
    pub(super) quiet: bool,
    pub(super) set: bool,
    pub(super) unset: bool,
    pub(super) options: Vec<String>,
}

impl crate::args::BpafArgs for ShoptCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let set_o_names_only = bpaf::short('o').help("Manage set -o options.").switch();
        let print = bpaf::short('p')
            .help("Print options' current values.")
            .switch();
        let quiet = bpaf::short('q').help("Suppress typical output.").switch();
        let set = bpaf::short('s').help("Set the specified options.").switch();
        let unset = bpaf::short('u')
            .help("Unset the specified options.")
            .switch();
        let options = bpaf::positional::<String>("OPTIONS")
            .help("Names of options to operate on.")
            .many();

        bpaf::construct!(ShoptCommand {
            set_o_names_only,
            print,
            quiet,
            set,
            unset,
            options,
        })
    }
fn about() -> &'static str {
        "Manage shopt-style options."
    }
fn synopsis() -> &'static str {
        "[-opqsu] [OPTIONS]..."
    }
}

impl FromArgs for ShoptCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ShoptCommand {
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
