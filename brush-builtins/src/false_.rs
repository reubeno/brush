use brush_core::{ExecutionResult, builtins};

/// Return exit code 1.
pub(crate) struct FalseCommand {}

impl builtins::HelpContent for FalseCommand {
    fn synopsis(_name: &str) -> String {
        "false".into()
    }

    fn description(_name: &str) -> String {
        "fail".into()
    }

    fn detailed_help(
        _name: &str,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok("Returns a failure exit status.".into())
    }
}

impl builtins::FromArgs for FalseCommand {
    fn from_args(
        _name: &str,
        _args: Vec<brush_core::CommandArg>,
    ) -> Result<Self, builtins::ArgsError> {
        Ok(Self {})
    }
}

impl builtins::Command for FalseCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::general_error())
    }
}
