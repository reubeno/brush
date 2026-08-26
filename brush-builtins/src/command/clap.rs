//! `command` builtin: `CommandCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Directly invokes an external command, without going through typical search order.
#[derive(Default, Parser)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[arg(short = 'p')]
    pub(crate) use_default_path: bool,

    /// Display a short description of the command.
    #[arg(short = 'v')]
    pub(crate) print_description: bool,

    /// Display a more verbose description of the command.
    #[arg(short = 'V')]
    pub(crate) print_verbose_description: bool,

    /// Command and arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command_and_args: Vec<String>,
}

impl builtins::Command for CommandCommand {
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
