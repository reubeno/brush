//! `trap` builtin: `TrapCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage signal traps.
#[derive(Parser)]
pub(crate) struct TrapCommand {
    /// List all signal names.
    #[arg(short = 'l')]
    pub(super) list_signals: bool,

    /// Print registered trap commands.
    #[arg(short = 'p')]
    pub(super) print_trap_commands: bool,

    pub(super) args: Vec<String>,
}

impl builtins::Command for TrapCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
