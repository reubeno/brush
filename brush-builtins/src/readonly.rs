use clap::Parser;

use brush_core::builtins;

use crate::declare::{DeclareCommand, DeclareVerb, MakeAssociativeArrayFlag, MakeIndexedArrayFlag};

/// Mark shell variables or functions as read-only.
#[derive(Parser)]
pub(crate) struct ReadonlyCommand {
    /// Names are treated as function names.
    #[arg(short = 'f')]
    names_are_functions: bool,

    /// Display all read-only names.
    #[arg(short = 'p')]
    display_readonly_names: bool,

    /// Make the variable an indexed array when assigning to it.
    #[arg(short = 'a')]
    make_indexed_array: bool,

    /// Make the variable an associative array when assigning to it.
    #[arg(short = 'A')]
    make_associative_array: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[clap(skip)]
    declarations: Vec<brush_core::CommandArg>,
}

impl builtins::DeclarationCommand for ReadonlyCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for ReadonlyCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // `readonly` is `declare` with the readonly attribute implied by the verb and only a
        // subset of its options accepted; this struct exists so that subset is what the command
        // line takes.
        DeclareCommand {
            function_names_or_defs_only: self.names_are_functions,
            print: self.display_readonly_names,
            make_indexed_array: MakeIndexedArrayFlag::new(self.make_indexed_array.then_some(true)),
            make_associative_array: MakeAssociativeArrayFlag::new(
                self.make_associative_array.then_some(true),
            ),
            ..DeclareCommand::default()
        }
        .execute_as(DeclareVerb::Readonly, &self.declarations, context)
        .await
    }
}
