use brush_core::{ExecutionExitCode, builtins, trace_categories};

/// (UNIMPLEMENTED COMMAND)
#[derive(usage::Cli)]
#[usage(bin = "unimp", unknown_flags = "value", args_override_self = false)]
pub(crate) struct UnimplementedCommand {
    #[usage(arg, double_dash = "automatic")]
    args: Vec<String>,
}

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

brush_core::impl_usage_parse!(UnimplementedCommand);
