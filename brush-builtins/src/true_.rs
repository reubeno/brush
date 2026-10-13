use brush_core::ExecutionResult;
use brush_core::builtins::{self, Help, Invocation, ParseArgs, UsageError};

/// No-op command. Same with :.
pub(crate) struct TrueCommand;

// Takes no options and ignores its arguments, `--help` included, as bash's does.
impl ParseArgs for TrueCommand {
    fn help(_name: &str) -> Help {
        Help::new("true", "Return a successful result.")
            .with_details("Exit Status:\nAlways succeeds.")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for TrueCommand {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}
