use bpaf::Parser;
use brush_core::{ExecutionExitCode, builtins, trace_categories};

/// (UNIMPLEMENTED COMMAND)
pub(crate) struct UnimplementedCommand {
    args: Vec<String>,
}

impl builtins::Command for UnimplementedCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // Capture all arguments verbatim; no option parsing is performed.
        let args = bpaf::any("ARGS", Some).many();
        bpaf::construct!(UnimplementedCommand { args })
    }

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
