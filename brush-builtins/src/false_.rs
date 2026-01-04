use brush_core::{ExecutionResult, builtins, error};

/// Return exit code 1.
pub(crate) struct FalseCommand {}

impl builtins::SimpleCommand for FalseCommand {
    fn get_content(
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

    fn execute<I: Iterator<Item = S>, S: AsRef<str>, SR: brush_core::ShellRuntime>(
        _context: brush_core::ExecutionContext<'_, SR>,
        _args: I,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::general_error())
    }
}
