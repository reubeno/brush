use brush_core::{ExecutionExitCode, builtins, trace_categories};

use usage::Cli;

/// (UNIMPLEMENTED COMMAND)
#[derive(Cli)]
#[usage(bin = "unimp", unknown_flags = "value", args_override_self = false)]
pub(crate) struct UnimplementedCommand {
    #[usage(arg, double_dash = "automatic")]
    args: Vec<String>,
}

brush_builtin_usage::usage_builtin!(UnimplementedCommand);

impl builtins::Command for UnimplementedCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
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
