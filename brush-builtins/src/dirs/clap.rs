//! `dirs` builtin: `DirsCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Manage the current directory stack.
#[derive(Default, Parser)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[arg(short = 'c')]
    pub(super) clear: bool,

    /// Don't tilde-shorten paths.
    #[arg(short = 'l')]
    pub(super) tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[arg(short = 'p')]
    pub(super) print_one_per_line: bool,

    /// Print one directory per line with its index.
    #[arg(short = 'v')]
    pub(super) print_one_per_line_with_index: bool,
    //
    // TODO(dirs): implement +N and -N
}

impl builtins::Command for DirsCommand {
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
