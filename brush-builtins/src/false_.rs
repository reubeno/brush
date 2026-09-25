use brush_core::{ExecutionResult, builtins};

/// Return exit code 1.
pub(crate) struct FalseCommand {}

brush_builtin_utils::verbatim_builtin!(
    FalseCommand,
    synopsis = "false",
    description = "fail",
    help = "Returns a failure exit status.\n",
);

impl builtins::Command for FalseCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::general_error())
    }
}
