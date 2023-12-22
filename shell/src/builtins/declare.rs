use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct DeclareCommand {
    #[arg(short = 'f')]
    function_names_or_defs_only: bool,

    #[arg(short = 'F')]
    function_names_only: bool,

    #[arg(short = 'g')]
    create_global: bool,

    #[arg(short = 'I')]
    locals_inherit_from_prev_scope: bool,

    #[arg(short = 'p')]
    print: bool,

    //
    // Attribute options
    //
    // TODO: allow + to be used to disable option
    //
    #[arg(short = 'a')]
    make_indexed_array: bool,

    #[arg(short = 'A')]
    make_associative_array: bool,

    #[arg(short = 'i')]
    make_integer: bool,

    #[arg(short = 'l')]
    lowercase_value_on_assignment: bool,

    #[arg(short = 'n')]
    make_nameref: bool,

    #[arg(short = 'r')]
    make_readonly: bool,

    #[arg(short = 't')]
    make_traced: bool,

    #[arg(short = 'u')]
    uppercase_value_on_assignment: bool,

    #[arg(short = 'x')]
    make_exported: bool,

    #[arg(name = "name[=value]")]
    names: Vec<String>,
}

impl BuiltinCommand for DeclareCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        let _as_local = context.builtin_name == "local";

        log::error!(
            "UNIMPLEMENTED: {}: built-in unimplemented: {:?}",
            context
                .shell
                .shell_name
                .as_ref()
                .map_or("(unknown shell)", |sn| sn),
            self.names,
        );
        Ok(BuiltinExitCode::Unimplemented)
    }
}
