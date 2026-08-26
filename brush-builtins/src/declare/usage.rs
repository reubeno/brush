//! `declare` builtin: `DeclareCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use itertools::Itertools;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

crate::usage_minus_or_plus_flag_arg!(MakeIndexedArrayFlag, 'i', "+i", "Make the variable an indexed array.");
crate::usage_minus_or_plus_flag_arg!(MakeAssociativeArrayFlag, 'A', "+A", "Make the variable an associative array.");
crate::usage_minus_or_plus_flag_arg!(CapitalizeValueOnAssignmentFlag, 'c', "+c", "Enable capitalize-on-assignment for the variable.");
crate::usage_minus_or_plus_flag_arg!(LowercaseValueOnAssignmentFlag, 'l', "+l", "Assign values in lowercase.");
crate::usage_minus_or_plus_flag_arg!(MakeExportedFlag, 'x', "+x", "Export the variable.");
crate::usage_minus_or_plus_flag_arg!(MakeIntegerFlag, 'i', "+i", "Make the variable an integer.");
crate::usage_minus_or_plus_flag_arg!(MakeNameRefFlag, 'n', "+n", "Make the variable a name reference.");
crate::usage_minus_or_plus_flag_arg!(MakeReadonlyFlag, 'r', "+r", "Make the variable readonly.");
crate::usage_minus_or_plus_flag_arg!(MakeTracedFlag, 't', "+t", "Enable tracing for the variable.");
crate::usage_minus_or_plus_flag_arg!(UppercaseValueOnAssignmentFlag, 'u', "+u", "Assign values in uppercase.");

/// Display or update variables and their attributes.
#[derive(usage::Cli)]
#[usage(
    bin = "declare",
    unknown_flags = "error",
    args_override_self = false,
    usage = "declare [OPTIONS] [DECLARATIONS]..."
)]
pub(crate) struct DeclareCommand {
    /// Constrain to function names or definitions.
    #[usage(short = 'f')]
    pub(super) function_names_or_defs_only: bool,

    /// Constrain to function names only.
    #[usage(short = 'F')]
    pub(super) function_names_only: bool,

    /// Create global variable, if applicable.
    #[usage(short = 'g')]
    pub(super) create_global: bool,

    /// When creating a local variable that shadows another variable of the same name,
    /// then initialize it with the contents and attributes of the variable being shadowed.
    #[usage(short = 'I')]
    pub(super) locals_inherit_from_prev_scope: bool,

    /// Display each item's attributes and values.
    #[usage(short = 'p')]
    pub(super) print: bool,

    //
    // Attribute options
    #[usage(flatten)] // -a
    pub(super) make_indexed_array: MakeIndexedArrayFlag,
    #[usage(flatten)] // -A
    pub(super) make_associative_array: MakeAssociativeArrayFlag,
    #[usage(flatten)] // -c
    pub(super) capitalize_value_on_assignment: CapitalizeValueOnAssignmentFlag,
    #[usage(flatten)] // -i
    pub(super) make_integer: MakeIntegerFlag,
    #[usage(flatten)] // -l
    pub(super) lowercase_value_on_assignment: LowercaseValueOnAssignmentFlag,
    #[usage(flatten)] // -n
    pub(super) make_nameref: MakeNameRefFlag,
    #[usage(flatten)] // -r
    pub(super) make_readonly: MakeReadonlyFlag,
    #[usage(flatten)] // -t
    pub(super) make_traced: MakeTracedFlag,
    #[usage(flatten)] // -u
    pub(super) uppercase_value_on_assignment: UppercaseValueOnAssignmentFlag,
    #[usage(flatten)] // -x
    pub(super) make_exported: MakeExportedFlag,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[usage(skip)]
    pub(super) declarations: Vec<brush_core::CommandArg>,
}

crate::impl_usage_parse!(DeclareCommand);

impl FromArgs for DeclareCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for DeclareCommand {
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

impl builtins::DeclarationCommand for DeclareCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }

}
