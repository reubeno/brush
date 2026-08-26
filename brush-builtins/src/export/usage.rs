//! `export` builtin: `ExportCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Add or update exported shell variables.
#[derive(usage::Cli)]
#[usage(bin = "export", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ExportCommand {
    /// Names are treated as function names.
    #[usage(short = 'f')]
    pub(super) names_are_functions: bool,

    /// Un-export the names.
    #[usage(short = 'n')]
    pub(super) unexport: bool,

    /// Display all exported names.
    #[usage(short = 'p')]
    pub(super) display_exported_names: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by usage, but filled in by the BuiltinDeclarationCommand trait.
    #[usage(skip)]
    pub(super) declarations: Vec<brush_core::CommandArg>,
}

crate::impl_usage_parse!(ExportCommand);

impl FromArgs for ExportCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ExportCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}

impl builtins::DeclarationCommand for ExportCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }
}
