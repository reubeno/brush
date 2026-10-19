use brush_core::{ExecutionResult, builtins, error};

/// No-op command. Same with :.
pub(crate) struct TrueCommand {}

impl builtins::ParseArgs for TrueCommand {
    fn content(
        _name: &str,
        content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => Ok("Returns a successful exit status.".into()),
            builtins::ContentType::ShortUsage => Ok("true".into()),
            builtins::ContentType::ShortDescription => Ok("true - success".into()),
            builtins::ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }

    fn parse(_invocation: builtins::Invocation<'_>) -> Result<Self, builtins::UsageError> {
        Ok(Self {})
    }
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for TrueCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
