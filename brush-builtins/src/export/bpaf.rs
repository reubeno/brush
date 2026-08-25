//! `export` builtin: `ExportCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

fn display_all_exported_vars(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
) -> Result<(), brush_core::Error> {
    // Enumerate variables, sorted by key.
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if variable.is_exported() {
            let value = variable.value().try_get_cow_str(context.shell);
            if let Some(value) = value {
                writeln!(context.stdout(), "declare -x {name}=\"{value}\"")?;
            } else {
                writeln!(context.stdout(), "declare -x {name}")?;
            }
        }
    }

    Ok(())
}

/// Add or update exported shell variables.
pub(crate) struct ExportCommand {
    /// Names are treated as function names.
    pub(super) names_are_functions: bool,

    /// Un-export the names.
    pub(super) unexport: bool,

    /// Display all exported names.
    #[expect(dead_code)]
    pub(super) display_exported_names: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by the parser, but filled in by the
    // BuiltinDeclarationCommand trait.
    pub(super) declarations: Vec<brush_core::CommandArg>,
}

impl crate::args::BpafArgs for ExportCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let names_are_functions = bpaf::short('f')
            .help("Names are treated as function names.")
            .switch();
        let unexport = bpaf::short('n').help("Un-export the names.").switch();
        let display_exported_names = bpaf::short('p')
            .help("Display all exported names.")
            .switch();

        // N.B. Declarations are captured separately from options.
        let declarations = bpaf::pure(Vec::new());

        bpaf::construct!(ExportCommand {
            names_are_functions,
            unexport,
            display_exported_names,
            declarations,
        })
    }
fn about() -> &'static str {
        "Add or update exported shell variables."
    }
fn synopsis() -> &'static str {
        "[-fn] [NAME[=VALUE]]..."
    }
}

impl FromArgs for ExportCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ExportCommand {
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

impl builtins::DeclarationCommand for ExportCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }

}
