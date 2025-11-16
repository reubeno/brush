use brush_core::{ExecutionResult, builtins, error};

/// No-op command.
pub(crate) struct ColonCommand {}

impl builtins::SimpleCommand for ColonCommand {
    fn get_content(
        _name: &str,
        content_type: builtins::ContentType,
    ) -> Result<String, brush_core::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => {
                Ok("Null command; always returns success.".into())
            }
            builtins::ContentType::ShortUsage => Ok(":: :".into()),
            builtins::ContentType::ShortDescription => Ok(": - Null command".into()),
            builtins::ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }

    fn execute<I: Iterator<Item = S>, S: AsRef<str>>(
        _context: brush_core::ExecutionContext<'_>,
        _args: I,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}
