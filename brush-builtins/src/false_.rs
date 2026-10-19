use brush_core::{ExecutionResult, builtins, error};

/// Return exit code 1.
pub(crate) struct FalseCommand {}

impl builtins::ParseArgs for FalseCommand {
    fn content(
        _name: &str,
        content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => Ok("Returns a failure exit status.".into()),
            builtins::ContentType::ShortUsage => Ok("false".into()),
            builtins::ContentType::ShortDescription => Ok("false - fail".into()),
            builtins::ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }

    fn parse(_invocation: builtins::Invocation<'_>) -> Result<Self, builtins::UsageError> {
        Ok(Self {})
    }
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for FalseCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::general_error())
    }
}
