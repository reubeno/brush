use brush_core::{ExecutionResult, builtins};

/// No-op command.
pub(crate) struct ColonCommand {}

brush_builtin_utils::verbatim_builtin!(
    ColonCommand,
    synopsis = ":",
    description = "Null command",
    help = "Null command; always returns success.\n",
);

impl builtins::Command for ColonCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
