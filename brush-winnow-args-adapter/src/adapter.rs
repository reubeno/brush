//! Parses builtin arguments with winnow-args and renders their help.

use std::fmt::Write as _;

use brush_core::CommandArg;
use brush_core::builtins::{ContentOptions, ContentType, Invocation, UsageError};
use winnow_args::BStr;
use winnow_args::color::{Ansi, Depth, Paint, Palette};
use winnow_args::{Args, Error, ErrorKind, help};

/// The colors of brush's help: yellow headings, magenta flags and cyan value
/// names, with clap's colors for errors.
pub const HELP_PALETTE: Palette = Palette {
    header: Paint::ansi(Ansi::Yellow).bold().underline(),
    flag: Paint::ansi(Ansi::Magenta).bold(),
    command: Paint::ansi(Ansi::Magenta).bold(),
    placeholder: Paint::ansi(Ansi::Cyan),
    ..Palette::CLAP
};

/// Parses a builtin's arguments, each flattened to a word.
///
/// # Errors
///
/// Returns a [`UsageError`] for a parse failure, or with the help text when
/// the arguments ask for help or version information.
pub fn parse<T: Args>(invocation: Invocation<'_>) -> Result<T, UsageError> {
    let name = invocation.name();
    parse_words(name, &into_words(invocation.into_args()))
}

/// Like [`parse`], but leaves the first `--` and everything after it unparsed.
///
/// Returns them for the builtin to read (`echo`, `test`, `set`, `getopts`).
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the words before the separator.
pub fn parse_with_double_dash<T: Args>(
    invocation: Invocation<'_>,
) -> Result<(T, Vec<String>), UsageError> {
    let name = invocation.name();
    let mut words = into_words(invocation.into_args());
    let Some(separator) = words.iter().position(|w| w == "--") else {
        return Ok((parse_words(name, &words)?, Vec::new()));
    };
    let trailing = words.split_off(separator);
    Ok((parse_words(name, &words)?, trailing))
}

/// Parses a declaration utility's arguments: its options, and the rest as
/// declarations, which it returns as they arrived.
///
/// The options are the words that start with `-` or `+`, and aren't only
/// that, up to the first word that doesn't or to `--`, which is dropped. No
/// declaration utility's option takes its value as a separate word.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the options.
pub fn parse_declarations<T: Args>(
    invocation: Invocation<'_>,
) -> Result<(T, Vec<CommandArg>), UsageError> {
    let name = invocation.name();
    let mut args = invocation.into_args();
    let end = args
        .iter()
        .position(|arg| match arg {
            CommandArg::String(s) => s == "--" || !(s.len() > 1 && s.starts_with(['-', '+'])),
            CommandArg::Assignment(_) => true,
        })
        .unwrap_or(args.len());

    let mut declarations = args.split_off(end);
    if matches!(declarations.first(), Some(CommandArg::String(s)) if s == "--") {
        declarations.remove(0);
    }

    Ok((parse_words(name, &into_words(args))?, declarations))
}

/// Flattens arguments into words. Assignments render as `name=value`.
fn into_words(args: Vec<CommandArg>) -> Vec<String> {
    args.into_iter().map(String::from).collect()
}

/// Builtin invocations this short borrow their words from a stack buffer
/// rather than an allocated list.
const INLINE_WORDS: usize = 16;

fn parse_words<T: Args>(name: &str, words: &[String]) -> Result<T, UsageError> {
    let parsed = if words.len() <= INLINE_WORDS {
        let mut buf = [BStr::new(b""); INLINE_WORDS];
        for (slot, word) in buf.iter_mut().zip(words) {
            *slot = BStr::new(word.as_bytes());
        }
        T::parse_words(&buf[..words.len()])
    } else {
        T::parse_words(&winnow_args::words(words))
    };
    parsed.map_err(|e| to_usage_error::<T>(name, &e))
}

/// A parse failure as the builtin reports it.
///
/// That is `name: error: …` in winnow-args' words, then the usage line. A
/// help request is [`UsageError::help`], with the help of the subcommand it
/// was made in: the short page for `-h` and the long one for `--help`, as
/// clap's. The shell prints it on standard output. A bare invocation of an
/// `arg_required_else_help` command is instead a plain usage error with that
/// help. A version request is the top-level help.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
/// * `error` - The failure.
pub fn to_usage_error<T: Args>(name: &str, error: &Error) -> UsageError {
    match error.kind() {
        ErrorKind::HelpRequested | ErrorKind::VersionRequested => {
            let text = error
                .render_help_styled(name, help::Style::PLAIN)
                .unwrap_or_else(|| {
                    // Only a version request has no help page of its own.
                    help::render_styled(T::HELP, &[name], true, help::width(), help::Style::PLAIN)
                });
            if error.is_bare_help() {
                UsageError::new(text.trim_end())
            } else {
                UsageError::help(text.trim_end())
            }
        }
        _ => UsageError::new(format!(
            "{name}: error: {}\nusage: {}",
            error.message(help::Style::PLAIN),
            synopsis::<T>(name)
        )),
    }
}

/// Returns the builtin's help content of the given type.
///
/// # Errors
///
/// Returns an error for [`ContentType::ManPage`], which isn't implemented.
pub fn content<T: Args>(
    name: &str,
    content_type: ContentType,
    options: &ContentOptions,
) -> Result<String, brush_core::Error> {
    Ok(match content_type {
        ContentType::DetailedHelp => {
            let style = if options.colorized {
                help::Style::at(HELP_PALETTE, Depth::Ansi16)
            } else {
                help::Style::PLAIN
            };
            help::render_styled(T::HELP, &[name], true, help::width(), style)
        }
        ContentType::ShortUsage => format!("{name}: {}\n", synopsis::<T>(name)),
        ContentType::ShortDescription => format!("{name} - {}\n", T::HELP.about),
        ContentType::ManPage => {
            brush_core::error::unimp("man page rendering is not yet implemented")?
        }
    })
}

/// A one-line synopsis: `name [-ab] [-c value] [arg ...]`, from the help data
/// the derive emits: short and `+` options; long options and hidden flags
/// are left out.
fn synopsis<T: Args>(name: &str) -> String {
    let command: &help::Command = T::HELP;
    let mut out = String::from(name);
    let visible = || command.items.iter().filter(|i| !i.hide);
    let switch = |i: &&help::Item| !i.positional && i.value_name.is_none() && !i.required;
    let switches: String = visible()
        .filter(switch)
        .flat_map(|i| i.short.into_iter().chain(i.more_shorts.iter().copied()))
        .collect();
    if !switches.is_empty() {
        let _ = write!(out, " [-{switches}]");
    }
    let plus_switches: String = visible().filter(switch).filter_map(|i| i.plus).collect();
    if !plus_switches.is_empty() {
        let _ = write!(out, " [+{plus_switches}]");
    }
    for item in visible().filter(|i| !i.positional && i.value_name.is_some()) {
        let value = item.value_name.unwrap_or("VALUE").to_lowercase();
        let value = if item.optional_value {
            format!("[{value}]")
        } else {
            value
        };
        // `-o value` and `+o value` are told apart; a long-only flag is left out.
        let spellings = item
            .short
            .map(|c| format!("-{c}"))
            .into_iter()
            .chain(item.plus.map(|c| format!("+{c}")));
        for flag in spellings {
            if item.required {
                let _ = write!(out, " {flag} {value}");
            } else {
                let _ = write!(out, " [{flag} {value}]");
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    use brush_core::builtins::{DeclarationRole, ParseArgs};

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

    crate::impl_parse_args!(EchoLike, keep_double_dash = args);

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

    crate::impl_parse_args!(ExportLike, declarations = declarations);

    /// Set-shaped: `+x` disables what `-x` enables, including in a cluster.
    #[derive(Debug, winnow_args::Args)]
    #[arg(plus_options, disable_help_flag, disable_version_flag)]
    struct PlusLike {
        #[arg(short = 'x', plus = 'x')]
        x: Option<bool>,
        #[arg(short = 'y', plus = 'y')]
        y: Option<bool>,
        /// Options end at the first operand.
        #[arg(positional, stop_flags)]
        words: Vec<String>,
    }

    crate::impl_parse_args!(PlusLike);

    /// Help-shaped: `--help` gives the help text.
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

    crate::impl_parse_args!(HelpLike);

    /// Subcommand-shaped: each subcommand has a help page of its own.
    #[derive(Debug, winnow_args::Args)]
    #[arg(arg_required_else_help)]
    struct CtlLike {
        #[arg(subcommand)]
        command: CtlCommand,
    }

    #[derive(Debug, winnow_args::Subcommand)]
    enum CtlCommand {
        /// Show the status.
        Status,
        /// Enable a thing.
        Enable {
            /// The thing to enable.
            #[arg(positional)]
            thing: String,
        },
    }

    crate::impl_parse_args!(CtlLike);

    fn words(parsed: Vec<CommandArg>) -> Vec<String> {
        into_words(parsed)
    }

    #[test]
    fn a_double_dash_and_what_follows_are_kept() {
        let parsed =
            <EchoLike as ParseArgs>::parse(Invocation::new("echo", ["-n", "--", "-e"])).unwrap();
        assert!(parsed.no_newline);
        assert_eq!(parsed.args, ["--", "-e"]);
    }

    #[test]
    fn an_unknown_flag_becomes_an_operand() {
        let parsed = <EchoLike as ParseArgs>::parse(Invocation::new("echo", ["-z", "hi"])).unwrap();
        assert!(!parsed.no_newline);
        assert_eq!(parsed.args, ["-z", "hi"]);
    }

    #[test]
    fn declarations_start_at_the_first_operand() {
        let parsed =
            <ExportLike as ParseArgs>::parse(Invocation::new("export", ["-n", "FOO", "-p"]))
                .unwrap();
        assert!(parsed.unexport);
        assert_eq!(ExportLike::DECLARATION_ROLE, DeclarationRole::Utility);
        assert_eq!(words(parsed.declarations), ["FOO", "-p"]);
    }

    #[test]
    fn declarations_start_after_a_double_dash_which_is_dropped() {
        let parsed =
            <ExportLike as ParseArgs>::parse(Invocation::new("export", ["-n", "--", "-n"]))
                .unwrap();
        assert!(parsed.unexport);
        assert_eq!(words(parsed.declarations), ["-n"]);
    }

    #[test]
    fn a_lone_dash_or_plus_is_a_declaration() {
        let parsed =
            <ExportLike as ParseArgs>::parse(Invocation::new("export", ["-", "+"])).unwrap();
        assert!(!parsed.unexport);
        assert_eq!(words(parsed.declarations), ["-", "+"]);
    }

    #[test]
    fn a_plus_cluster_toggles_each_flag_and_options_stop_at_operands() {
        let disabled = <PlusLike as ParseArgs>::parse(Invocation::new("set", ["+xy"])).unwrap();
        assert_eq!((disabled.x, disabled.y), (Some(false), Some(false)));

        let mixed =
            <PlusLike as ParseArgs>::parse(Invocation::new("set", ["-x", "foo", "+bar"])).unwrap();
        assert_eq!(mixed.x, Some(true));
        assert_eq!(mixed.words, ["foo", "+bar"]);
    }

    #[test]
    fn a_help_request_gives_the_help_text() {
        let err = <HelpLike as ParseArgs>::parse(Invocation::new("cd", ["--help"])).unwrap_err();
        assert!(err.is_help(), "{err}");
        assert!(err.to_string().contains("Directory to enter."), "{err}");
        let err = <HelpLike as ParseArgs>::parse(Invocation::new("cd", ["-z"])).unwrap_err();
        assert!(!err.is_help(), "{err}");
    }

    #[test]
    fn a_subcommand_and_its_arguments_parse() {
        let parsed =
            <CtlLike as ParseArgs>::parse(Invocation::new("ctl", ["enable", "x"])).unwrap();
        assert!(matches!(parsed.command, CtlCommand::Enable { thing } if thing == "x"));
    }

    #[test]
    fn a_help_request_in_a_subcommand_gives_its_help() {
        let err = <CtlLike as ParseArgs>::parse(Invocation::new("ctl", ["enable", "--help"]))
            .unwrap_err();
        assert!(err.is_help(), "{err}");
        assert!(err.to_string().contains("Usage: ctl enable "), "{err}");
        assert!(err.to_string().contains("The thing to enable."), "{err}");
        assert!(!err.to_string().contains("Show the status."), "{err}");
    }

    #[test]
    fn a_bare_invocation_that_needs_more_is_an_error_with_the_help() {
        let err =
            <CtlLike as ParseArgs>::parse(Invocation::new("ctl", [] as [&str; 0])).unwrap_err();
        assert!(!err.is_help(), "{err}");
        assert!(err.to_string().contains("Show the status."), "{err}");
    }

    #[test]
    fn an_unknown_option_is_reported_with_the_usage_line() {
        let err = <HelpLike as ParseArgs>::parse(Invocation::new("cd", ["-x"])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "cd: error: unknown flag `-x`\nusage: cd [-P] [target]"
        );
    }

    #[test]
    fn short_usage_and_description_come_from_the_help_data() {
        let options = ContentOptions::default();
        let usage = HelpLike::content("cd", ContentType::ShortUsage, &options).unwrap();
        assert_eq!(usage, "cd: cd [-P] [target]\n");
        let description = HelpLike::content("cd", ContentType::ShortDescription, &options).unwrap();
        assert_eq!(
            description,
            "cd - Help-shaped: `--help` gives the help text.\n"
        );
    }

    #[test]
    fn colorized_help_uses_brush_colors() {
        let options = ContentOptions { colorized: true };
        let help = HelpLike::content("cd", ContentType::DetailedHelp, &options).unwrap();
        assert!(help.contains("\x1b[1;4;33mUsage:\x1b[0m"), "{help:?}");
        assert!(help.contains("\x1b[1;35m-P\x1b[0m"), "{help:?}");
        assert!(help.contains("\x1b[36mTARGET\x1b[0m"), "{help:?}");
    }
}
