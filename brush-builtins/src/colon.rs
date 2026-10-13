use brush_core::ExecutionResult;
use brush_core::builtins::{self, Help, Invocation, ParseArgs, UsageError};

/// No-op command.
pub(crate) struct ColonCommand;

// Takes no options and ignores its arguments, `--help` included, as bash's does.
impl ParseArgs for ColonCommand {
    fn help(_name: &str) -> Help {
        Help::new(":", "Null command.")
            .with_details("No effect; the command does nothing.\n\nExit Status:\nAlways succeeds.")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for ColonCommand {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}
