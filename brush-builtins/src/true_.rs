use brush_core::{ExecutionResult, builtins};

/// No-op command. Same with :.
pub(crate) struct TrueCommand {}

brush_builtin_utils::verbatim_builtin!(
    TrueCommand,
    synopsis = "true",
    description = "success",
    help = "Returns a successful exit status.\n",
);

impl builtins::Command for TrueCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
