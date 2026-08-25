//! `trap` builtin: `TrapCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Manage signal traps.
#[derive(Bpaf)]
pub(crate) struct TrapCommand {
    /// List all signal names.
    #[bpaf(short('l'))]
    pub(super) list_signals: bool,

    /// Print registered trap commands.
    #[bpaf(short('p'))]
    pub(super) print_trap_commands: bool,

    /// Handler command and signals to operate on.
    #[bpaf(positional("ARGS"))]
    pub(super) args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for TrapCommand {
fn parser() -> impl bpaf::Parser<Self> {
        trap_command()
    }
fn about() -> &'static str {
        "Manage signal traps."
    }
fn synopsis() -> &'static str {
        "[-lp] [ARGS]..."
    }
}

impl FromArgs for TrapCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for TrapCommand {
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
