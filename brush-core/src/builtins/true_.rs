use crate::{builtins, commands, error};

/// Command that always returns success.
pub(crate) struct TrueCommand {}

impl builtins::SimpleCommand for TrueCommand {
    fn get_content(
        _name: &str,
        content_type: builtins::ContentType,
    ) -> Result<String, crate::error::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => Ok("Always returns success.".into()),
            builtins::ContentType::ShortUsage => Ok("true: true".into()),
            builtins::ContentType::ShortDescription => Ok("true - Return success.".into()),
            builtins::ContentType::ManPage => error::unimp("man page not yet implemented"),
        }
    }

    fn execute<I: Iterator<Item = S>, S: AsRef<str>>(
        _context: commands::ExecutionContext<'_>,
        _args: I,
    ) -> Result<builtins::BuiltinResult, crate::error::Error> {
        Ok(builtins::BuiltinResult {
            exit_code: builtins::ExitCode::Success,
        })
    }
}
