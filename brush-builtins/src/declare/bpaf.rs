//! `declare` builtin: `DeclareCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use itertools::Itertools;
use std::{io::Write, sync::LazyLock};
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

#[derive(Clone, Copy)]
enum DeclareVerb {
    Declare,
    Local,
    Readonly,
}

/// Display or update variables and their attributes.
pub(crate) struct DeclareCommand {
    pub(super) function_names_or_defs_only: bool,
    pub(super) function_names_only: bool,
    pub(super) create_global: bool,
    pub(super) locals_inherit_from_prev_scope: bool,
    pub(super) print: bool,

    // Attribute options
    pub(super) make_indexed_array: Option<bool>,
    pub(super) make_associative_array: Option<bool>,
    pub(super) capitalize_value_on_assignment: Option<bool>,
    pub(super) make_integer: Option<bool>,
    pub(super) lowercase_value_on_assignment: Option<bool>,
    pub(super) make_nameref: Option<bool>,
    pub(super) make_readonly: Option<bool>,
    pub(super) make_traced: Option<bool>,
    pub(super) uppercase_value_on_assignment: Option<bool>,
    pub(super) make_exported: Option<bool>,

    // N.B. These are skipped during parsing, but filled in by the
    // DeclarationCommand trait.
    pub(super) declarations: Vec<brush_core::CommandArg>,
}

impl crate::args::BpafArgs for DeclareCommand {
fn takes_plus_options() -> bool {
        true
    }
fn parser() -> impl bpaf::Parser<Self> {
        let function_names_or_defs_only = bpaf::short('f')
            .help("Constrain to function names or definitions.")
            .switch();
        let function_names_only = bpaf::short('F')
            .help("Constrain to function names only.")
            .switch();
        let create_global = bpaf::short('g')
            .help("Create global variable, if applicable.")
            .switch();
        let locals_inherit_from_prev_scope = bpaf::short('I')
            .help(
                "When creating a local variable that shadows another variable of the same name, \
                 then initialize it with the contents and attributes of the variable being \
                 shadowed.",
            )
            .switch();
        let print = bpaf::short('p')
            .help("Display each item's attributes and values.")
            .switch();

        let make_indexed_array =
            crate::minus_or_plus_flag('a', "+a", "Make the variable an indexed array.");
        let make_associative_array =
            crate::minus_or_plus_flag('A', "+A", "Make the variable an associative array.");
        let capitalize_value_on_assignment = crate::minus_or_plus_flag(
            'c',
            "+c",
            "Enable capitalize-on-assignment for the variable.",
        );
        let make_integer =
            crate::minus_or_plus_flag('i', "+i", "Mark the variable as integer-typed");
        let lowercase_value_on_assignment = crate::minus_or_plus_flag(
            'l',
            "+l",
            "Enable lowercase-on-assignment for the variable.",
        );
        let make_nameref =
            crate::minus_or_plus_flag('n', "+n", "Mark the variable as a name reference");
        let make_readonly = crate::minus_or_plus_flag('r', "+r", "Mark the variable as read-only.");
        let make_traced = crate::minus_or_plus_flag('t', "+t", "Enable tracing for the variable.");
        let uppercase_value_on_assignment = crate::minus_or_plus_flag(
            'u',
            "+u",
            "Enable uppercase-on-assignment for the variable.",
        );
        let make_exported = crate::minus_or_plus_flag('x', "+x", "Mark the variable for export.");

        let declarations = bpaf::pure(Vec::new());

        bpaf::construct!(DeclareCommand {
            function_names_or_defs_only,
            function_names_only,
            create_global,
            locals_inherit_from_prev_scope,
            print,
            make_indexed_array,
            make_associative_array,
            capitalize_value_on_assignment,
            make_integer,
            lowercase_value_on_assignment,
            make_nameref,
            make_readonly,
            make_traced,
            uppercase_value_on_assignment,
            make_exported,
            declarations,
        })
    }
fn about() -> &'static str {
        "Display or update variables and their attributes."
    }
fn synopsis() -> &'static str {
        "[OPTIONS] [DECLARATIONS]..."
    }
}

impl FromArgs for DeclareCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for DeclareCommand {
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
