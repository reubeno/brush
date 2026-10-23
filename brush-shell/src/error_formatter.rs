use std::borrow::Cow;

/// Formats the shell's errors: `$0: message` in an interactive shell (or
/// `$0: file: line N: message` for a syntax error in a file), and otherwise
/// `file: line N: message`, naming the file the error arose in
/// (`BASH_SOURCE[0]`), or `$0` for commands not read from one.
#[derive(Debug, Default, Clone)]
pub(crate) struct Formatter {
    pub use_color: bool,
}

impl brush_core::extensions::ErrorFormatter for Formatter {
    fn format_error(
        &self,
        err: &brush_core::error::Error,
        shell: &brush_core::Shell<impl brush_core::ShellExtensions>,
    ) -> String {
        let shell_name = || shell.current_shell_name().unwrap_or(Cow::Borrowed("brush"));
        let mut line_shown = false;
        // A background job of an interactive shell reports as the shell does.
        let prefix = if shell.started_interactive() {
            // A syntax error in a file still names the file and the line. A parse of
            // text from partway into its source (as `eval`'s or a trap handler's is)
            // isn't a file's, whatever its source is called.
            match (err.kind(), err.location()) {
                (
                    brush_core::ErrorKind::ParseError(_, parsed),
                    Some(brush_core::error::ErrorLocation {
                        source,
                        line: Some(line),
                    }),
                ) if parsed.start.is_none() && !source.is_empty() => {
                    line_shown = true;
                    format!("{}: {source}: line {line}: ", shell_name())
                }
                _ => format!("{}: ", shell_name()),
            }
        } else {
            let current;
            let location = if let Some(location) = err.location() {
                location
            } else {
                current = shell.current_location();
                &current
            };
            let name = if location.source.is_empty() {
                shell_name()
            } else {
                Cow::Borrowed(location.source.as_str())
            };
            match location.line {
                Some(line) => {
                    line_shown = true;
                    format!("{name}: line {line}: ")
                }
                None => format!("{name}: "),
            }
        };
        let message = match err.kind() {
            // A parse error names its source, which the prefix does already.
            brush_core::ErrorKind::ParseError(parse_error, _) => {
                parse_error_message(parse_error, line_shown)
            }
            _ => std::format!("{err:#}"),
        };
        if self.use_color {
            std::format!(
                "{}{message}\n",
                color_print::cformat!("<red>{prefix}</red>")
            )
        } else {
            std::format!("{prefix}{message}\n")
        }
    }
}

/// Returns a parse error's message. The parser numbers lines from the start of
/// the text it parsed, which isn't the file's line for `eval`'s text, say; so
/// once the prefix shows the line, the message gives only the column.
fn parse_error_message(parse_error: &brush_core::parser::ParseError, line_shown: bool) -> String {
    if !line_shown {
        return parse_error.to_string();
    }
    match parse_error {
        brush_core::parser::ParseError::ParsingNear(position) => {
            format!("syntax error at column {}", position.column)
        }
        brush_core::parser::ParseError::Tokenizing {
            inner,
            position: Some(position),
        } => format!("{inner} (detected near column {})", position.column),
        brush_core::parser::ParseError::Tokenizing { position: None, .. }
        | brush_core::parser::ParseError::ParsingAtEndOfInput => parse_error.to_string(),
    }
}
