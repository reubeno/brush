//! `enable` builtin: `EnableCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Enable, disable, or display built-in commands.
#[derive(Parser)]
pub(crate) struct EnableCommand {
    /// Print a list of built-in commands.
    #[arg(short = 'a')]
    pub(super) print_list: bool,

    /// Disables the specified built-in commands.
    #[arg(short = 'n')]
    pub(super) disable: bool,

    /// Print a list of built-in commands with reusable output.
    #[arg(short = 'p')]
    pub(super) print_reusably: bool,

    /// Only operate on special built-in commands.
    #[arg(short = 's')]
    pub(super) special_only: bool,

    /// Path to a shared object from which built-in commands will be loaded.
    #[arg(short = 'f', value_name = "PATH")]
    pub(super) shared_object_path: Option<String>,

    /// Remove the built-in commands loaded from the indicated object path.
    #[arg(short = 'd')]
    pub(super) remove_loaded_builtin: bool,

    /// Names of built-in commands to operate on.
    pub(super) names: Vec<String>,
}

impl builtins::Command for EnableCommand {
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
