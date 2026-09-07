//! `fc` builtin: `FcCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use brush_core::builtins;
use clap::Parser;

/// Process command history list.
#[derive(Parser)]
pub(crate) struct FcCommand {
    /// List commands instead of editing them.
    #[arg(short = 'l')]
    pub(super) list: bool,

    /// Suppress line numbers when listing.
    #[arg(short = 'n', requires = "list")]
    pub(super) no_line_numbers: bool,

    /// Reverse the order of commands.
    #[arg(short = 'r')]
    pub(super) reverse: bool,

    /// Re-execute command after substitution (old=new format).
    #[arg(short = 's')]
    pub(super) substitute: bool,

    /// Editor to use (only relevant when not listing or substituting).
    #[arg(short = 'e', value_name = "ENAME")]
    pub(super) editor: Option<String>,

    /// First command in range (number or string prefix).
    #[arg(value_name = "FIRST", allow_hyphen_values = true)]
    pub(super) first: Option<String>,

    /// Last command in range (number or string prefix).
    #[arg(value_name = "LAST", allow_hyphen_values = true)]
    pub(super) last: Option<String>,
}

impl builtins::Command for FcCommand {
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
