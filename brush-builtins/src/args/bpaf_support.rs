//! bpaf-engine runtime support for engine-side builtin argument modules.
//!
//! Each `{builtin}/bpaf.rs` implements [`BpafArgs`] (parser construction plus
//! the shell-specific knobs) for its command type; the helpers here run those
//! parsers against invocation words and render help, mirroring the semantics
//! of the clap-side engine.
#![cfg(all(feature = "parser-bpaf", not(feature = "parser-usage")))]

use bpaf::Parser;
use std::ffi::OsStr;

use brush_core::args::ArgsError;

/// Trait implemented by `{builtin}/bpaf.rs` for each builtin's argument type.
pub trait BpafArgs: Sized {
    /// Returns the parser used to interpret the command's arguments.
    fn parser() -> impl bpaf::Parser<Self> + 'static;

    /// Returns a short, one-line description of the command.
    fn about() -> &'static str {
        ""
    }

    /// Returns a short synopsis of the command's arguments.
    fn synopsis() -> &'static str {
        ""
    }

    /// Returns whether the command takes options with a leading '+' character.
    fn takes_plus_options() -> bool {
        false
    }

    /// Returns whether the command captures all remaining arguments verbatim
    /// after its options.
    fn takes_trailing_args() -> bool {
        false
    }

    /// Returns the short options that take a value; used when deciding where
    /// the option section ends for commands with trailing arguments.
    fn value_taking_short_options() -> &'static str {
        ""
    }

    /// Stores trailing (verbatim) arguments captured during parsing.
    fn set_trailing_args(&mut self, _args: Vec<String>) {}

    /// Parses the invocation words (including the command name at index 0)
    /// into an instance of the implementing type.
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let mut args: Vec<String> = words.to_vec();

        // N.B. The first word is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let args = if Self::takes_plus_options() {
            expand_plus_option_groups(args)
        } else {
            args
        };

        if Self::takes_trailing_args() {
            // N.B. The trailing section keeps any leading `--`; commands see
            // it and decide, mirroring how the clap side handles it.
            let (options, trailing) =
                split_option_section(&args, Self::value_taking_short_options(), &[]);

            let mut command = run_parser::<Self>(&options)?;
            command.set_trailing_args(trailing);

            Ok(command)
        } else {
            run_parser::<Self>(&args)
        }
    }
}

pub(crate) fn render_parse_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        // Help/version requests are rendered to stdout with a success exit code.
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        // Everything else is a usage error.
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Parses only an option section (already stripped of the command name);
/// used by declaration-style builtins whose operands are handled separately.
#[expect(dead_code, reason = "reserved for declaration-style builtins")]
pub(crate) fn parse_options_only<T: BpafArgs>(mut options: Vec<String>) -> Result<T, ArgsError> {
    if T::takes_plus_options() {
        options = expand_plus_option_groups(options);
    }

    run_parser::<T>(&options)
}

/// Runs the given command's parser against the provided words.
pub fn run_parser<T: BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_parse_failure)
}

/// Expands groups of plus-style options (e.g., `+vx`) into individually
/// recognizable tokens (e.g., `+v` and `+x`). Tokens that do not start with a
/// single `+` are passed through unchanged.
pub fn expand_plus_option_groups(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .flat_map(|arg| {
            if let Some(plus_options) = arg.strip_prefix('+').filter(|g| !g.is_empty()) {
                if plus_options.starts_with('+') || plus_options.contains('=') {
                    vec![arg]
                } else {
                    plus_options.chars().map(|c| format!("+{c}")).collect()
                }
            } else {
                vec![arg]
            }
        })
        .collect()
}

/// Returns whether the given token looks like a long option, i.e., `--`
/// followed by a name of word characters, optionally with an attached
/// `=value`.
fn is_long_option(arg: &str) -> bool {
    let Some(long) = arg.strip_prefix("--").filter(|l| !l.is_empty()) else {
        return false;
    };

    let name = long.split('=').next().unwrap_or("");
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_');

    first_ok
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Returns whether the given token looks like a group of short (or
/// plus-style) options.
fn is_short_or_plus_option(arg: &str, lead: char, value_shorts: &str) -> bool {
    let Some(group) = arg.strip_prefix(lead) else {
        return false;
    };

    let mut saw_value_taker = false;
    for c in group.chars() {
        if c.is_whitespace() {
            return false;
        }

        if saw_value_taker {
            continue;
        }

        if value_shorts.contains(c) {
            saw_value_taker = true;
        } else if !c.is_alphabetic() {
            return false;
        }
    }

    !group.is_empty()
}

/// Returns the number of tokens occupied by a short (or plus-style) option
/// group: one if any value-taking option in the group has its value attached
/// or takes no value, and two if the last option takes a separate value.
fn short_group_token_count(group: &str, value_shorts: &str) -> usize {
    let char_count = group.chars().count();
    for (j, c) in group.chars().enumerate() {
        if value_shorts.contains(c) {
            return if j == char_count - 1 { 2 } else { 1 };
        }
    }

    1
}

/// Splits an argument list into the leading section of options (to be parsed)
/// and the trailing section of operands (captured verbatim), following
/// shell-style option termination rules:
///
/// * Parsing stops at the first `--`, which is dropped from the options and placed at the front of
///   the trailing section.
/// * Parsing stops at the first operand, which starts the trailing section.
/// * `value_shorts` characters consume a following value; long options in `value_longs` likewise.
pub fn split_option_section(
    args: &[String],
    value_shorts: &str,
    value_longs: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();

        if arg == "--" {
            return (args[..i].to_vec(), args[i..].to_vec());
        }

        if is_long_option(arg) {
            if value_longs.contains(&arg) {
                i += 2;
            } else {
                i += 1;
            }
        } else if is_short_or_plus_option(arg, '-', value_shorts) {
            i += short_group_token_count(arg.strip_prefix('-').unwrap_or(""), value_shorts);
        } else if is_short_or_plus_option(arg, '+', value_shorts) {
            i += short_group_token_count(arg.strip_prefix('+').unwrap_or(""), value_shorts);
        } else {
            return (args[..i].to_vec(), args[i..].to_vec());
        }
    }

    (args.to_vec(), Vec::new())
}

/// Renders help content for a bpaf-backed builtin.
pub fn get_content<T: BpafArgs>(
    name: &str,
    content_type: &brush_core::builtins::ContentType,
    _options: &brush_core::builtins::ContentOptions,
) -> Result<String, brush_core::error::Error> {
    use brush_core::builtins::ContentType;

    match content_type {
        ContentType::DetailedHelp => {
            // N.B. Trigger bpaf's --help handling to render help content,
            // since rendered help text is not otherwise exposed.
            let help_args = [OsStr::new("--help")];
            let help_request = bpaf::Args::from(&help_args[..]).set_name(name);
            match T::parser().to_options().run_inner(help_request) {
                Err(failure) => Ok(render_parse_failure(failure).message),
                Ok(_) => Err(brush_core::error::ErrorKind::Unimplemented(
                    "unexpectedly parsed help request",
                )
                .into()),
            }
        }
        ContentType::ShortUsage => Ok(format!("{name}: {name} {}\n", T::synopsis())),
        ContentType::ShortDescription => Ok(format!("{name} - {}\n", T::about())),
        ContentType::ManPage => {
            brush_core::error::unimp("man page rendering is not yet implemented")
        }
    }
}
