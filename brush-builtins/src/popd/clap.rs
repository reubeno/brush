//! `popd` builtin: `PopdCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Pop a path from the current directory stack.
#[derive(Parser)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[clap(short = 'n')]
    pub(super) no_directory_change: bool,
    //
    // TODO(popd): implement +N and -N
}

impl builtins::Command for PopdCommand {
    type Error = crate::dirs::DirError;

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
