//! Parses builtin arguments with winnow-args and renders their help.

use std::fmt::Write as _;

use brush_core::CommandArg;
use brush_core::builtins::{ArgsError, ContentOptions};
use brush_core::error;
use winnow_args::{Args, Error, ErrorKind, help};

/// Parses a builtin's arguments.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under, for its messages.
/// * `args` - The arguments following `name`.
///
/// # Errors
///
/// Returns [`ArgsError::HelpRequested`] when the arguments ask for help or
/// version information, and [`ArgsError::Usage`] for any other parse failure.
pub fn parse<T: Args>(name: &str, args: Vec<CommandArg>) -> Result<T, ArgsError> {
    parse_words(name, &brush_builtin_utils::into_words(args))
}

/// Like [`parse`], but leaves the first `--` and everything after it unparsed.
///
/// Returns them for the builtin to read (`echo`, `test`, `set`, `getopts`).
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the words before the separator.
pub fn parse_with_trailing<T: Args>(
    name: &str,
    args: Vec<CommandArg>,
) -> Result<(T, Vec<String>), ArgsError> {
    let mut words = brush_builtin_utils::into_words(args);
    let Some(separator) = words.iter().position(|w| w == "--") else {
        return Ok((parse_words(name, &words)?, Vec::new()));
    };
    let trailing = words.split_off(separator);
    Ok((parse_words(name, &words)?, trailing))
}

/// Like [`parse`], but parses only the leading options.
///
/// Returns the operands after them untouched, so declarations keep their
/// assignment form. The split is [`brush_builtin_utils::split_leading_options`].
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the leading options.
pub fn parse_with_declarations<T: Args>(
    name: &str,
    args: Vec<CommandArg>,
) -> Result<(T, Vec<CommandArg>), ArgsError> {
    let (options, operands) = brush_builtin_utils::split_leading_options(args);
    Ok((parse_words(name, &options)?, operands))
}

fn parse_words<T: Args>(name: &str, words: &[String]) -> Result<T, ArgsError> {
    T::parse_words(&winnow_args::words(words)).map_err(|e| to_args_error::<T>(name, &e))
}

/// An error as bash prints it for a builtin: `name: -x: invalid option`, then
/// `name: usage: name [-ab] [arg ...]`; a missing operand gets the usage line
/// alone.
fn to_args_error<T: Args>(name: &str, error: &Error) -> ArgsError {
    let usage = format!("{name}: usage: {}", synopsis::<T>(name));
    let token = error.token().unwrap_or_default();
    let what = match error.kind() {
        ErrorKind::HelpRequested | ErrorKind::VersionRequested => {
            return ArgsError::HelpRequested(help_text::<T>(name));
        }
        ErrorKind::MissingArgument | ErrorKind::MissingRequired => {
            return ArgsError::Usage(usage);
        }
        ErrorKind::UnknownFlag => format!("{token}: invalid option"),
        ErrorKind::MissingValue => format!("{token}: option requires an argument"),
        _ => error.to_string(),
    };
    ArgsError::Usage(format!("{name}: {what}\n{usage}"))
}

/// `name --help` as bash prints it: the synopsis, then the description
/// indented.
fn help_text<T: Args>(name: &str) -> String {
    let mut out = format!("{name}: {}", synopsis::<T>(name));
    let command = T::HELP;
    let text = if command.long_about.is_empty() {
        command.about
    } else {
        command.long_about
    };
    for line in text.lines() {
        let _ = write!(out, "\n    {line}");
    }
    out
}

/// bash's one-line synopsis: `name [-ab] [-c value] [arg ...]`, from the help
/// data the derive emits. Long options and hidden flags (`+x` forms) are left
/// out, as `help -s` shows them.
pub fn synopsis<T: Args>(name: &str) -> String {
    let command: &help::Command = T::HELP;
    let mut out = String::from(name);
    let visible = || command.items.iter().filter(|i| !i.hide);
    let switches: String = visible()
        .filter(|i| !i.positional && i.value_name.is_none() && !i.required)
        .filter_map(|i| i.short)
        .collect();
    if !switches.is_empty() {
        let _ = write!(out, " [-{switches}]");
    }
    for item in visible().filter(|i| !i.positional && i.value_name.is_some()) {
        let Some(short) = item.short else { continue };
        let value = item.value_name.unwrap_or("VALUE").to_lowercase();
        if item.required {
            let _ = write!(out, " -{short} {value}");
        } else {
            let _ = write!(out, " [-{short} {value}]");
        }
    }
    for item in visible().filter(|i| i.positional) {
        let value = item.value_name.unwrap_or("arg").to_lowercase();
        let dots = if item.multiple { " ..." } else { "" };
        if item.required {
            let _ = write!(out, " {value}{dots}");
        } else {
            let _ = write!(out, " [{value}{dots}]");
        }
    }
    out
}

/// Returns the type's about text.
pub fn description<T: Args>(_: &str) -> String {
    T::HELP.about.to_owned()
}

/// Renders the type's full help.
///
/// # Errors
///
/// Never fails; the signature matches [`brush_core::builtins::HelpContent::detailed_help`].
pub fn detailed_help<T: Args>(
    name: &str,
    options: &ContentOptions,
) -> Result<String, error::Error> {
    let style = if options.colorized {
        help::Style::COLORED
    } else {
        help::Style::PLAIN
    };
    Ok(help::render_styled(
        T::HELP,
        &[name],
        true,
        help::width(),
        style,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use brush_core::builtins::{FromArgs, HelpContent};

    fn strings(words: &[&str]) -> Vec<CommandArg> {
        words
            .iter()
            .map(|word| CommandArg::String((*word).to_owned()))
            .collect()
    }

    /// Echo-shaped: unknown flags are operands, and `--` stays in the tail.
    #[derive(Debug, winnow_args::Args)]
    #[arg(unknown_flags = "value", disable_help_flag, disable_version_flag)]
    struct EchoLike {
        /// Suppress the trailing newline.
        #[arg(short = 'n')]
        no_newline: bool,

        /// Words to echo.
        #[arg(positional)]
        args: Vec<String>,
    }

    crate::winnow_builtin!(EchoLike, trailing_args = args);

    /// Export-shaped: options stop at the first operand.
    #[derive(Debug, winnow_args::Args)]
    #[arg(disable_help_flag, disable_version_flag)]
    struct ExportLike {
        /// Un-export the names.
        #[arg(short = 'n')]
        unexport: bool,

        /// Operands, filled by the adapter.
        #[arg(skip)]
        declarations: Vec<CommandArg>,
    }

    crate::winnow_builtin!(ExportLike, declarations = declarations);

    /// Set-shaped: `+x` disables what `-x` enables, including in a cluster.
    #[derive(Debug, winnow_args::Args)]
    #[arg(plus_options, disable_help_flag, disable_version_flag)]
    struct PlusLike {
        #[arg(short = 'x', plus = 'x')]
        x: Option<bool>,
        #[arg(short = 'y', plus = 'y')]
        y: Option<bool>,
        /// Options end at the first operand, as for bash's `set`.
        #[arg(positional, stop_flags)]
        words: Vec<String>,
    }

    crate::winnow_builtin!(PlusLike);

    /// Help-shaped: `--help` is a request, not a usage failure.
    #[derive(Debug, winnow_args::Args)]
    #[arg(disable_version_flag)]
    struct HelpLike {
        /// Use the physical directory.
        #[arg(short = 'P')]
        physical: bool,

        /// Directory to enter.
        #[arg(positional)]
        target: Option<String>,
    }

    crate::winnow_builtin!(HelpLike);

    #[test]
    #[allow(clippy::panic)]
    fn trailing_separator_is_preserved() {
        let parsed = EchoLike::from_args("echo", strings(&["-n", "--", "-e"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert!(parsed.no_newline);
        assert_eq!(parsed.args, ["--", "-e"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn unknown_flag_becomes_an_operand() {
        let parsed = EchoLike::from_args("echo", strings(&["-z", "hi"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert!(!parsed.no_newline);
        assert_eq!(parsed.args, ["-z", "hi"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn declarations_stop_at_the_first_operand() {
        let parsed = ExportLike::from_args("export", strings(&["-n", "FOO", "-p"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert!(parsed.unexport);
        const { assert!(ExportLike::TAKES_DECLARATIONS) };
        assert_eq!(
            brush_builtin_utils::into_words(parsed.declarations),
            ["FOO", "-p"]
        );
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_cluster_toggles_each_flag_and_stops_at_operands() {
        let disabled = PlusLike::from_args("set", strings(&["+xy"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert_eq!((disabled.x, disabled.y), (Some(false), Some(false)));

        let mixed = PlusLike::from_args("set", strings(&["-x", "foo", "+bar"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert_eq!(mixed.x, Some(true));
        assert_eq!(mixed.words, ["foo", "+bar"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn help_request_is_not_a_usage_error() {
        match HelpLike::from_args("cd", strings(&["--help"])) {
            Err(ArgsError::HelpRequested(text)) => assert!(text.contains("cd"), "{text}"),
            Err(err) => panic!("unexpected argument error: {err}"),
            Ok(_) => panic!("`--help` asks for help"),
        }
    }

    #[test]
    #[allow(clippy::panic)]
    fn unknown_option_is_a_usage_error_in_bash_form() {
        match HelpLike::from_args("cd", strings(&["-x"])) {
            Err(ArgsError::Usage(text)) => {
                assert_eq!(text, "cd: -x: invalid option\ncd: usage: cd [-P] [target]");
            }
            Err(err) => panic!("unexpected argument error: {err}"),
            Ok(_) => panic!("an unknown option is a usage error"),
        }
    }

    #[test]
    fn synopsis_and_description_come_from_the_help_data() {
        assert_eq!(HelpLike::synopsis("cd"), "cd [-P] [target]");
        assert_eq!(
            HelpLike::description("cd"),
            "Help-shaped: `--help` is a request, not a usage failure."
        );
    }
}
