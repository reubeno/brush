use brush_core::{ExecutionResult, builtins};

/// No-op command. Same with :.
pub(crate) struct TrueCommand {}

impl builtins::HelpContent for TrueCommand {
    fn synopsis(_name: &str) -> String {
        "true".into()
    }

    fn description(_name: &str) -> String {
        "success".into()
    }

    fn detailed_help(
        _name: &str,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok("Returns a successful exit status.".into())
    }
}

impl builtins::FromArgs for TrueCommand {
    fn from_args(
        _name: &str,
        _args: Vec<brush_core::CommandArg>,
    ) -> Result<Self, builtins::ArgsError> {
        Ok(Self {})
    }
}

impl builtins::Command for TrueCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::success())
    }
}
