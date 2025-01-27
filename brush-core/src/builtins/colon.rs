use crate::{builtins, commands, error};

/// No-op command.
pub(crate) struct ColonCommand {}

impl builtins::SimpleCommand for ColonCommand {
    fn get_content(
        _name: &str,
        content_type: builtins::ContentType,
    ) -> Result<String, crate::error::Error> {
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
        _context: commands::ExecutionContext<'_>,
        _args: I,
    ) -> Result<builtins::BuiltinResult, crate::error::Error> {
        Ok(builtins::BuiltinResult {
            exit_code: builtins::ExitCode::Success,
        })
    }
}
