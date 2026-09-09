use clap::Parser;

use brush_core::builtins;

use crate::declare::{DeclareCommand, DeclareVerb};

/// Create and update local variables inside a shell function.
#[derive(Parser)]
#[clap(override_usage = "local [OPTIONS] [DECLARATIONS]...")]
pub(crate) struct LocalCommand {
    /// `local` takes every option `declare` does; only the scope rules differ.
    #[clap(flatten)]
    declare: DeclareCommand,
}

impl builtins::DeclarationCommand for LocalCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declare.set_declarations(declarations);
    }
}

impl builtins::Command for LocalCommand {
    fn takes_plus_options() -> bool {
        true
    }

    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        self.declare
            .execute_as(DeclareVerb::Local, &self.declare.declarations, context)
            .await
    }
}
