use brush_core::{ExecutionExitCode, builtins, trace_categories};

use clap::Parser;

/// (UNIMPLEMENTED COMMAND)
#[derive(Parser)]
pub(crate) struct UnimplementedCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(UnimplementedCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for UnimplementedCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        tracing::warn!(target: trace_categories::UNIMPLEMENTED,
            "unimplemented built-in: {} {}",
            context.command_name,
            self.args.join(" ")
        );
        Ok(ExecutionExitCode::Unimplemented.into())
    }
}
