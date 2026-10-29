use brush_core::{ExecutionResult, builtins, error};

/// No-op command.
pub(crate) struct ColonCommand {}

impl builtins::ParseArgs for ColonCommand {
    fn content(
        name: &str,
        content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => {
                Ok("Null command; always returns success.".into())
            }
            builtins::ContentType::ShortUsage => Ok(format!("{name}: {name}\n")),
            builtins::ContentType::ShortDescription => Ok(format!("{name} - Null command\n")),
            builtins::ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }

    fn parse(_invocation: builtins::Invocation<'_>) -> Result<Self, builtins::UsageError> {
        Ok(Self {})
    }
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ColonCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
