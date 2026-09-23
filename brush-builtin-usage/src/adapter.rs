//! Parses builtin arguments with usage-rs and renders their help.
//!
//! usage's derive generates inherent methods rather than a trait, so
//! [`UsageParsed`] is the bound this module can name. [`crate::usage_builtin`]
//! implements it by forwarding to those methods.

use std::ffi::{OsStr, OsString};

use brush_core::CommandArg;
use brush_core::builtins::{ArgsError, ContentOptions};
use brush_core::error;

/// Bridge from a `#[derive(usage::Cli)]` type to this adapter.
///
/// Implementations forward to the inherent methods the derive generates.
pub trait UsageParsed: Sized {
    /// Parses `argv`, which does not include the program name.
    ///
    /// # Errors
    ///
    /// Returns the usage-rs parse error, including help and version requests.
    fn parse_argv<'v>(argv: &[&'v OsStr]) -> Result<Self, usage::Error<'static, 'v>>;

    /// Returns the static spec generated for this type.
    fn spec() -> &'static usage::spec::Spec<'static>;

    /// Returns the static parse tables generated for this type.
    fn command() -> &'static usage::Command<'static>;
}

/// Parses a builtin's arguments with usage.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under. Usage and error text use
///   it, in place of the spec's compiled bin name.
/// * `args` - The arguments following `name`.
///
/// # Errors
///
/// Returns [`ArgsError::HelpRequested`] when the arguments ask for help or
/// version information, and [`ArgsError::Usage`] for any other parse failure.
pub fn parse<T: UsageParsed>(name: &str, args: Vec<CommandArg>) -> Result<T, ArgsError> {
    parse_words(name, &brush_builtin_utils::into_words(args))
}

/// Like [`parse`], but leaves the first `--` and everything after it unparsed.
///
/// Returns the separator and every word after it, or an empty remainder when no
/// separator is present. usage consumes `--` itself; builtins such as `echo`
/// and `test` need to see it.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the words before the separator.
pub fn parse_with_trailing<T: UsageParsed>(
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
/// assignment form. The split is [`brush_builtin_utils::split_leading_options`]:
/// the same boundary `clap_builtin!` uses for `declare` and `export`.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the leading options.
pub fn parse_with_declarations<T: UsageParsed>(
    name: &str,
    args: Vec<CommandArg>,
) -> Result<(T, Vec<CommandArg>), ArgsError> {
    let (options, operands) = brush_builtin_utils::split_leading_options(args);
    Ok((parse_words(name, &options)?, operands))
}

/// Builds a one-line synopsis from the type's declared short options and
/// positionals.
///
/// Long options are omitted, matching the synopsis `clap_builtin!` produces
/// for `help -s`. Hidden flags, including `+x` forms, are omitted too.
pub fn synopsis<T: UsageParsed>(name: &str) -> String {
    let command = T::command();
    let meta = T::spec().root;
    let mut optional_shorts = String::new();
    let mut required_shorts = String::new();

    for (flag, flag_meta) in command.flags.iter().copied().zip(meta.flags.iter()) {
        if flag_meta.hide || flag_meta.builtin {
            continue;
        }
        let Some(&short) = flag.shorts.first() else {
            continue;
        };
        let ch = char::from(short);
        if flag_meta.required {
            required_shorts.push(ch);
        } else {
            optional_shorts.push(ch);
        }
    }

    let mut parts = Vec::new();
    parts.push(name.to_owned());
    if !optional_shorts.is_empty() {
        parts.push(format!("[-{optional_shorts}]"));
    }
    if !required_shorts.is_empty() {
        parts.push(format!("-{required_shorts}"));
    }
    for (arg, arg_meta) in command.args.iter().copied().zip(meta.args.iter()) {
        if arg_meta.hide {
            continue;
        }
        let label = positional_label(arg, arg_meta);
        if arg.required {
            parts.push(label);
        } else {
            parts.push(format!("[{label}]"));
        }
    }

    parts.join(" ")
}

/// Returns the type's about text, from the spec or the root command.
pub fn description<T: UsageParsed>(_: &str) -> String {
    let spec = T::spec();
    spec.about.or(spec.root.about).unwrap_or("").to_owned()
}

/// Renders the type's full help with usage.
///
/// # Errors
///
/// Never fails; the signature matches [`brush_core::builtins::HelpContent::detailed_help`].
pub fn detailed_help<T: UsageParsed>(
    name: &str,
    options: &ContentOptions,
) -> Result<String, error::Error> {
    let spec = T::spec().view().name(name).bin(name).spec();
    let style = if options.colorized {
        usage::help::Style::COLOURED
    } else {
        usage::help::Style::PLAIN
    };
    Ok(render_help(&spec, T::command(), true, style))
}

fn parse_words<T: UsageParsed>(name: &str, words: &[String]) -> Result<T, ArgsError> {
    match rewrite_plus_options::<T, String>(words) {
        Some(renamed) => parse_os(name, &renamed),
        None => parse_os(name, words),
    }
}

fn parse_os<T: UsageParsed>(name: &str, words: &[String]) -> Result<T, ArgsError> {
    let os_args: Vec<OsString> = words
        .iter()
        .map(|word| OsString::from(word.as_str()))
        .collect();
    let refs: Vec<&OsStr> = os_args.iter().map(OsString::as_os_str).collect();
    match T::parse_argv(&refs) {
        Ok(parsed) => Ok(parsed),
        Err(err) => Err(to_args_error::<T>(name, &refs, &err)),
    }
}

fn to_args_error<T: UsageParsed>(
    name: &str,
    argv: &[&OsStr],
    err: &usage::Error<'_, '_>,
) -> ArgsError {
    let spec = T::spec().view().name(name).bin(name).spec();
    match err {
        usage::Error::Help { cmd, long } => {
            ArgsError::HelpRequested(render_help(&spec, cmd, *long, usage::help::Style::PLAIN))
        }
        usage::Error::HelpAll { cmd } => {
            ArgsError::HelpRequested(render_help(&spec, cmd, true, usage::help::Style::PLAIN))
        }
        usage::Error::Version { long } => {
            let text = if *long {
                spec.long_version.or(spec.version)
            } else {
                spec.version.or(spec.long_version)
            };
            ArgsError::HelpRequested(text.unwrap_or("").to_owned())
        }
        usage::Error::MissingArgsHelp { cmd } => {
            ArgsError::Usage(render_help(&spec, cmd, false, usage::help::Style::PLAIN))
        }
        _ => ArgsError::Usage(usage::render_failure_plain(&spec, argv, err)),
    }
}

fn render_help(
    spec: &usage::spec::Spec<'_>,
    cmd: &usage::Command<'_>,
    long: bool,
    style: usage::help::Style,
) -> String {
    usage::help::render_styled(spec, cmd, long, style).unwrap_or_default()
}

fn positional_label(arg: &usage::Arg<'_>, meta: &usage::spec::ArgMeta<'_>) -> String {
    if meta.value_names.is_empty() {
        arg.name.to_ascii_uppercase()
    } else {
        meta.value_names.join(" ")
    }
}

/// Renames options with a leading `+` (for example `+x`, `+ab`) to the
/// per-character long options (`--+x`, `--+a --+b`) that usage accepts.
///
/// Returns `None` when the command declares no such option or none appear, so
/// the caller can parse without copying. Renaming stops once operands begin,
/// so a `+`-prefixed operand survives intact (the `+bar` in `set -x foo +bar`).
fn rewrite_plus_options<T: UsageParsed, S: AsRef<str>>(words: &[S]) -> Option<Vec<String>> {
    if !words.iter().any(|word| word.as_ref().starts_with('+')) {
        return None;
    }
    if !declares_plus_option::<T>() {
        return None;
    }

    let command = T::command();
    let mut renamed = Vec::with_capacity(words.len());
    let mut parsing_operands = false;
    let mut expecting_value = false;

    for word in words.iter().map(AsRef::as_ref) {
        if parsing_operands || expecting_value {
            renamed.push(word.to_owned());
            expecting_value = false;
            continue;
        }

        if word == "-" || word == "--" || !word.starts_with(['-', '+']) {
            parsing_operands = true;
        }

        expecting_value = option_takes_value(command, word);

        if let Some(plus_options) = word.strip_prefix('+') {
            renamed.extend(plus_options.chars().map(|ch| format!("--+{ch}")));
        } else {
            renamed.push(word.to_owned());
        }
    }

    Some(renamed)
}

fn declares_plus_option<T: UsageParsed>() -> bool {
    T::command()
        .flags
        .iter()
        .any(|flag| flag.longs.iter().any(|long| long.starts_with('+')))
}

/// Returns whether `word` consumes the following word as its value.
///
/// Only the last option in a bundle (the `o` in `-ao` or `+ao`) can do so.
fn option_takes_value(command: &usage::Command<'_>, word: &str) -> bool {
    let flag = if let Some(plus_options) = word.strip_prefix('+') {
        plus_options
            .chars()
            .next_back()
            .and_then(|ch| find_long(command, &format!("+{ch}")))
    } else if let Some(long) = word.strip_prefix("--") {
        let name = long.split('=').next().unwrap_or(long);
        find_long(command, name)
    } else if let Some(shorts) = word.strip_prefix('-') {
        shorts
            .chars()
            .next_back()
            .and_then(|ch| find_short(command, ch))
    } else {
        None
    };

    flag.is_some_and(|flag| flag.takes_value)
}

fn find_long<'a>(command: &'a usage::Command<'a>, name: &str) -> Option<&'a usage::Flag<'a>> {
    command
        .flags
        .iter()
        .copied()
        .find(|flag| flag.longs.contains(&name))
}

fn find_short<'a>(command: &'a usage::Command<'a>, ch: char) -> Option<&'a usage::Flag<'a>> {
    let Ok(byte) = u8::try_from(ch) else {
        return None;
    };
    command
        .flags
        .iter()
        .copied()
        .find(|flag| flag.shorts.contains(&byte))
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
    #[derive(Debug, usage::Cli)]
    #[usage(
        bin = "echo",
        unknown_flags = "value",
        disable_help_flag,
        disable_version_flag
    )]
    struct EchoLike {
        /// Suppress the trailing newline.
        #[usage(short = 'n')]
        no_newline: bool,

        /// Words to echo.
        #[usage(arg)]
        args: Vec<String>,
    }

    crate::usage_builtin!(EchoLike, trailing_args = args);

    /// Export-shaped: options stop at the first operand.
    #[derive(Debug, usage::Cli)]
    #[usage(
        bin = "export",
        unknown_flags = "error",
        disable_help_flag,
        disable_version_flag
    )]
    struct ExportLike {
        /// Un-export the names.
        #[usage(short = 'n')]
        unexport: bool,

        /// Operands, filled by the adapter rather than by usage.
        #[usage(skip)]
        declarations: Vec<CommandArg>,
    }

    crate::usage_builtin!(ExportLike, declarations = declarations);

    /// Set-shaped: `+x` disables what `-x` enables, including in a cluster.
    #[derive(Debug, usage::Cli)]
    #[usage(bin = "set", disable_help_flag, disable_version_flag)]
    struct PlusLike {
        #[usage(short = 'x')]
        enable_x: bool,
        #[usage(long = "+x", hide)]
        disable_x: bool,
        #[usage(short = 'y')]
        enable_y: bool,
        #[usage(long = "+y", hide)]
        disable_y: bool,

        words: Vec<String>,
    }

    crate::usage_builtin!(PlusLike);

    /// Help-shaped: `--help` is a request, not a usage failure.
    #[derive(Debug, usage::Cli)]
    #[usage(bin = "cd", unknown_flags = "error")]
    struct HelpLike {
        /// Use the physical directory.
        #[usage(short = 'P')]
        physical: bool,

        /// Directory to enter.
        target: Option<String>,
    }

    crate::usage_builtin!(HelpLike);

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
    fn an_operand_hides_a_later_option() {
        let parsed = ExportLike::from_args("export", strings(&["FOO", "-n"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));

        assert!(!parsed.unexport);
        assert_eq!(
            brush_builtin_utils::into_words(parsed.declarations),
            ["FOO", "-n"]
        );
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_cluster_toggles_each_flag_and_stops_at_operands() {
        let disabled = PlusLike::from_args("set", strings(&["+xy"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert!(disabled.disable_x);
        assert!(disabled.disable_y);
        assert!(!disabled.enable_x);

        let mixed = PlusLike::from_args("set", strings(&["-x", "foo", "+bar"]))
            .unwrap_or_else(|err| panic!("parsing should succeed: {err}"));
        assert!(mixed.enable_x);
        assert!(!mixed.disable_x);
        assert_eq!(mixed.words, ["foo", "+bar"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn help_request_is_not_a_usage_error() {
        let Err(err) = HelpLike::from_args("cd", strings(&["--help"])) else {
            panic!("`--help` asks for help");
        };
        match err {
            ArgsError::HelpRequested(text) => {
                assert!(text.contains("cd"), "{text}");
            }
            ArgsError::Usage(text) => panic!("help was reported as a usage error: {text}"),
            _ => panic!("unexpected argument error: {err}"),
        }
    }

    #[test]
    #[allow(clippy::panic)]
    fn unknown_option_is_a_usage_error_named_for_the_invocation() {
        let Err(err) = HelpLike::from_args("builtin-cd", strings(&["--frobnicate"])) else {
            panic!("an unknown option is a usage error");
        };
        match err {
            ArgsError::Usage(text) => assert!(text.contains("builtin-cd"), "{text}"),
            ArgsError::HelpRequested(text) => panic!("usage error was reported as help: {text}"),
            _ => panic!("unexpected argument error: {err}"),
        }
    }

    #[test]
    fn synopsis_and_description_come_from_the_spec() {
        assert_eq!(HelpLike::synopsis("cd"), "cd [-P] [TARGET]");
        assert_eq!(
            HelpLike::description("cd"),
            "Help-shaped: `--help` is a request, not a usage failure."
        );
    }
}
