use brush_core::{ExecutionResult, builtins};

/// No-op command.
pub(crate) struct ColonCommand {}

impl builtins::HelpContent for ColonCommand {
    fn synopsis(_name: &str) -> String {
        ":".into()
    }

    fn description(_name: &str) -> String {
        "Null command".into()
    }

    fn detailed_help(
        _name: &str,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok("Null command; always returns success.".into())
    }
}

impl builtins::FromArgs for ColonCommand {
    fn from_args(
        _name: &str,
        _args: Vec<brush_core::CommandArg>,
    ) -> Result<Self, builtins::ArgsError> {
        Ok(Self {})
    }
}

impl builtins::Command for ColonCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
