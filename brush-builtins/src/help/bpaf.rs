//! `help` builtin: `HelpCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use itertools::Itertools;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

fn get_builtins_sorted_by_name<'a, SE: brush_core::ShellExtensions>(
    context: &'a brush_core::ExecutionContext<'_, SE>,
) -> Vec<(&'a String, &'a builtins::Registration<SE>)> {
    context
        .shell
        .builtins()
        .iter()
        .sorted_by_key(|(name, _)| *name)
        .collect()
}

/// Display command help.
pub(crate) struct HelpCommand {
    pub(super) short_description: bool,
    pub(super) man_page_style: bool,
    pub(super) short_usage: bool,
    pub(super) topic_patterns: Vec<String>,
}

impl crate::args::BpafArgs for HelpCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let short_description = bpaf::short('d')
            .help("Display a short description for the commands.")
            .switch();
        let man_page_style = bpaf::short('m')
            .help("Display a man-style page of documentation for the commands.")
            .switch();
        let short_usage = bpaf::short('s')
            .help("Display a short usage summary for the commands.")
            .switch();
        let topic_patterns = bpaf::positional::<String>("PATTERNS")
            .help("Patterns of topics to display help for.")
            .many();

        bpaf::construct!(HelpCommand {
            short_description,
            man_page_style,
            short_usage,
            topic_patterns,
        })
    }
fn about() -> &'static str {
        "Display command help."
    }
fn synopsis() -> &'static str {
        "[-dms] [PATTERNS]..."
    }
}

impl FromArgs for HelpCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for HelpCommand {
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
