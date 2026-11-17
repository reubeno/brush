use brush_core::{ExecutionExitCode, builtins, trace_categories};

/// (UNIMPLEMENTED COMMAND)
#[derive(winnow_args::Args)]
#[arg(
    unknown_flags = "value",
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct UnimplementedCommand {
    #[arg(positional, double_dash = "preserve")]
    args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(UnimplementedCommand);

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
