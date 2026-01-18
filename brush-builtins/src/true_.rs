use brush_core::{ExecutionResult, builtins, error};

/// No-op command. Same with :.
pub(crate) struct TrueCommand {}

impl builtins::SimpleCommand for TrueCommand {
    fn get_content(
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

    fn execute<SE: brush_core::ShellExtensions, I: Iterator<Item = S>, S: AsRef<str>>(
        _context: brush_core::ExecutionContext<'_, SE>,
        _args: I,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}
