use brush_core::ExecutionResult;
use brush_core::builtins::{self, Help, Invocation, ParseArgs, UsageError};

/// Return exit code 1.
pub(crate) struct FalseCommand;

// Takes no options and ignores its arguments, `--help` included, as bash's does.
impl ParseArgs for FalseCommand {
    fn help(_name: &str) -> Help {
        Help::new("false", "Return an unsuccessful result.")
            .with_details("Exit Status:\nAlways fails.")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for FalseCommand {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::general_error())
    }
}
