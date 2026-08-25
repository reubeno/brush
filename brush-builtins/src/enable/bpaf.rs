//! `enable` builtin: `EnableCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Bpaf;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Enable, disable, or display built-in commands.
#[derive(Bpaf)]
pub(crate) struct EnableCommand {
    /// Print a list of built-in commands.
    #[bpaf(short('a'))]
    pub(super) print_list: bool,

    /// Disables the specified built-in commands.
    #[bpaf(short('n'))]
    pub(super) disable: bool,

    /// Print a list of built-in commands with reusable output.
    #[bpaf(short('p'))]
    #[expect(dead_code)]
    pub(super) print_reusably: bool,

    /// Only operate on special built-in commands.
    #[bpaf(short('s'))]
    pub(super) special_only: bool,

    /// Path to a shared object from which built-in commands will be loaded.
    #[bpaf(short('f'), argument("PATH"))]
    pub(super) shared_object_path: Option<String>,

    /// Remove the built-in commands loaded from the indicated object path.
    #[bpaf(short('d'))]
    pub(super) remove_loaded_builtin: bool,

    /// Names of built-in commands to operate on.
    #[bpaf(positional("NAMES"))]
    pub(super) names: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for EnableCommand {
fn parser() -> impl bpaf::Parser<Self> {
        enable_command()
    }
fn about() -> &'static str {
        "Enable, disable, or display built-in commands."
    }
fn synopsis() -> &'static str {
        "[-adnps] [-f PATH] [NAMES]..."
    }
}

impl FromArgs for EnableCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for EnableCommand {
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
