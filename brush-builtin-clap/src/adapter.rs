//! Parses builtin arguments with clap and renders their help: what
//! [`crate::impl_parse_args`] expands to. Not part of the API, so it can change in
//! any release.

use clap::builder::styling;

use brush_core::builtins::{Help, Invocation, TerminalStyle, UsageError};

/// Classifies a clap failure, for a builtin that runs clap itself.
///
/// It's a help request; an unknown option or an option missing its value
/// (each reported as bash does); or another usage error, whose message is
/// clap's. The shell adds the builtin's name and usage line.
pub fn to_usage_error(err: &clap::Error) -> UsageError {
    use clap::error::{ContextKind, ErrorKind};

    let invalid_arg = || err.get(ContextKind::InvalidArg).map(ToString::to_string);
    match err.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayVersion
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => UsageError::help_requested(),
        ErrorKind::UnknownArgument => match invalid_arg() {
            Some(arg) if arg.starts_with(['-', '+']) => UsageError::invalid_option(as_given(&arg)),
            // A surplus operand.
            _ => UsageError::new(clap_message(err)),
        },
        // An option given no value: clap names it with its value's placeholder
        // (`-v <VAR>`), and the value it rejected is empty.
        ErrorKind::InvalidValue
            if err
                .get(ContextKind::InvalidValue)
                .is_none_or(|value| value.to_string().is_empty()) =>
        {
            match invalid_arg() {
                Some(arg) => {
                    let option = arg.split_whitespace().next().unwrap_or(&arg);
                    UsageError::requires_argument(as_given(option))
                }
                None => UsageError::new(clap_message(err)),
            }
        }
        // As in bash, a rejected value is reported without the usage line.
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => UsageError::new(clap_message(err)),
        _ => UsageError::new(clap_message(err)).with_usage_line(),
    }
}

/// Returns an option as it was given: a `+x` option reaches clap renamed to
/// `--+x` (see [`rewrite_plus_options`]).
fn as_given(option: &str) -> &str {
    option
        .strip_prefix("--")
        .filter(|option| option.starts_with('+'))
        .unwrap_or(option)
}

/// Returns clap's rendering of `err` up to its first blank line (so the names
/// a "required arguments were not provided" error lists are kept), without its
/// `error: ` prefix or what follows: the shell supplies the attribution and
/// the usage line.
fn clap_message(err: &clap::Error) -> String {
    let rendered = err.to_string();
    let message = rendered.split("\n\n").next().unwrap_or_default().trim_end();
    message
        .strip_prefix("error: ")
        .unwrap_or(message)
        .to_owned()
}

/// Parses a builtin's arguments with clap. Implements
/// [`brush_core::builtins::ParseArgs::parse`] for clap-derived types; see
/// [`crate::impl_parse_args`].
///
/// # Arguments
///
/// * `invocation` - The invocation; its name occupies clap's bin-name slot.
///
/// # Errors
///
/// Returns a [`UsageError`]: a help request if the arguments asked for help or
/// version information, and a usage error for any other parse failure.
pub fn parse<T: clap::Parser>(invocation: Invocation<'_>) -> Result<T, UsageError> {
    let name = invocation.name();
    parse_words(name, &invocation.into_words())
}

/// Like [`parse`], but leaves the first `--`, and every word after it,
/// unparsed.
///
/// Returns that `--` and every word after it, or an empty remainder when there
/// is none. See [`crate::impl_parse_args`]'s `keep_double_dash` option.
///
/// Exists because clap consumes `--` itself, while builtins such as `echo` and
/// `test` need to see it; see <https://github.com/clap-rs/clap/issues/5055>.
///
/// # Arguments
///
/// * `invocation` - The invocation.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the words before the separator.
pub fn parse_with_double_dash<T: clap::Parser>(
    invocation: Invocation<'_>,
) -> Result<(T, Vec<String>), UsageError> {
    let name = invocation.name();
    let mut words = invocation.into_words();

    let Some(separator) = words.iter().position(|word| word == "--") else {
        return Ok((parse_words(name, &words)?, Vec::new()));
    };

    let trailing = words.split_off(separator);
    Ok((parse_words(name, &words)?, trailing))
}

fn parse_words<T: clap::Parser>(name: &str, words: &[String]) -> Result<T, UsageError> {
    let invocation = std::iter::once(name.to_owned());
    match rewrite_plus_options::<T, String>(words) {
        Some(renamed) => T::try_parse_from(invocation.chain(renamed)),
        None => T::try_parse_from(invocation.chain(words.iter().cloned())),
    }
    .map_err(|e| {
        // Help for a subcommand isn't the builtin's help, which is all a help
        // request can print; print clap's, as a failure, as clap would.
        if e.kind() == clap::error::ErrorKind::DisplayHelp && names_subcommand::<T>(words) {
            UsageError::new(e.to_string().trim_end())
        } else {
            to_usage_error(&e)
        }
    })
}

/// Returns whether one of `words` names one of `T`'s subcommands.
fn names_subcommand<T: clap::Parser>(words: &[String]) -> bool {
    let command = T::command();
    words
        .iter()
        .any(|word| command.find_subcommand(word.as_str()).is_some())
}

/// Renames options with a leading '+' (e.g. `+x`, `+ab`) to the per-character
/// long options (`--+x`, `--+a --+b`) that clap accepts, since clap has no
/// notion of a '+'-prefixed option. Returns `None` when the command declares no
/// such options or none appear in `words`, letting the caller parse without
/// copying anything.
///
/// Renaming stops once operands begin, so a '+'-prefixed operand survives
/// intact (e.g. the `+bar` in `set -x foo +bar`).
fn rewrite_plus_options<T: clap::Parser, S: AsRef<str>>(words: &[S]) -> Option<Vec<String>> {
    if !words.iter().any(|w| w.as_ref().starts_with('+')) {
        return None;
    }

    let mut command = T::command();
    if !command
        .get_arguments()
        .any(|a| a.get_long().is_some_and(|l| l.starts_with('+')))
    {
        return None;
    }

    // Until it's built, an argument whose arity clap infers from its type
    // (e.g. a `Vec<String>` option) reports none, so its value would be taken
    // for the first operand.
    command.build();

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

        expecting_value = takes_next_word(&command, word);

        if let Some(plus_options) = word.strip_prefix('+') {
            renamed.extend(plus_options.chars().map(|c| format!("--+{c}")));
        } else {
            renamed.push(word.to_owned());
        }
    }

    Some(renamed)
}

/// Returns whether an option word takes the next word as its value: a long
/// option without `=value`, or a bundle of short (or `+`) options whose first
/// option that takes a value is its last, so the value isn't attached (`-o`,
/// `-ao`, but not `-oa`).
fn takes_next_word(command: &clap::Command, word: &str) -> bool {
    let takes_value = |arg: Option<&clap::Arg>| {
        arg.is_some_and(|a| a.get_num_args().is_some_and(|n| n.takes_values()))
    };
    if let Some(long) = word.strip_prefix("--") {
        return !long.contains('=') && takes_value(find_long(command, long));
    }
    let (options, plus) = if let Some(options) = word.strip_prefix('+') {
        (options, true)
    } else if let Some(options) = word.strip_prefix('-') {
        (options, false)
    } else {
        return false;
    };
    let mut chars = options.chars().peekable();
    while let Some(c) = chars.next() {
        let arg = if plus {
            find_long(command, format!("+{c}").as_str())
        } else {
            command.get_arguments().find(|a| a.get_short() == Some(c))
        };
        if takes_value(arg) {
            return chars.peek().is_none();
        }
    }
    false
}

fn find_long<'a>(command: &'a clap::Command, name: &str) -> Option<&'a clap::Arg> {
    command.get_arguments().find(|a| a.get_long() == Some(name))
}

/// Returns a clap-derived builtin's help.
///
/// That is a synopsis built from its declared options and positionals, its
/// `about` text, and clap's rendering of its arguments. Implements
/// [`brush_core::builtins::ParseArgs::help`]; see [`crate::impl_parse_args`].
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
pub fn help<T: clap::Parser>(name: &str) -> Help {
    render_help::<T>(name, Help::DETAILS_WIDTH, false)
}

/// Returns a clap-derived type's help rendered for a terminal of `style`:
/// [`help()`]'s text, wrapped to its width, with its sections painted by clap
/// in brush's colors.
pub fn terminal_help<T: clap::Parser>(name: &str, style: &TerminalStyle) -> Help {
    render_help::<T>(name, style.width, style.color)
}

fn render_help<T: clap::Parser>(name: &str, width: usize, styled: bool) -> Help {
    let command = help_command::<T>(name)
        .term_width(width)
        .max_term_width(width);
    let synopsis = short_usage(name, &command);
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();

    let mut arguments = command.clone().help_template("{all-args}{after-help}");
    let rendered = arguments.render_help();
    let sections = if styled {
        rendered.ansi().to_string()
    } else {
        rendered.to_string()
    };
    let sections = sections.trim_end();

    // The description's later paragraphs come first.
    let rest = command
        .get_long_about()
        .map(|long_about| crate::text::rest_of_description(&long_about.to_string(), width))
        .unwrap_or_default();
    let details = match (rest.is_empty(), sections.is_empty()) {
        (true, _) => sections.to_owned(),
        (false, true) => rest,
        (false, false) => format!("{rest}\n\n{sections}"),
    };

    Help::new(synopsis, about).with_details(details)
}

fn help_command<T: clap::Parser>(name: &str) -> clap::Command {
    let mut command = T::command()
        .styles(brush_help_styles())
        .next_line_help(false);
    command.set_bin_name(name);
    command
}

fn short_usage(name: &str, command: &clap::Command) -> String {
    let mut usage = String::new();

    let mut needs_space = false;

    let mut optional_short_opts = vec![];
    let mut required_short_opts = vec![];
    for opt in command.get_opts() {
        if opt.is_hide_set() {
            continue;
        }

        if let Some(c) = opt.get_short() {
            if !opt.is_required_set() {
                optional_short_opts.push(c);
            } else {
                required_short_opts.push(c);
            }
        }
    }

    if !optional_short_opts.is_empty() {
        if needs_space {
            usage.push(' ');
        }

        usage.push('[');
        usage.push('-');
        for c in optional_short_opts {
            usage.push(c);
        }

        usage.push(']');
        needs_space = true;
    }

    if !required_short_opts.is_empty() {
        if needs_space {
            usage.push(' ');
        }

        usage.push('-');
        for c in required_short_opts {
            usage.push(c);
        }

        needs_space = true;
    }

    for pos in command.get_positionals() {
        if pos.is_hide_set() {
            continue;
        }

        if !pos.is_required_set() {
            if needs_space {
                usage.push(' ');
            }

            usage.push('[');
            needs_space = false;
        }

        if let Some(names) = pos.get_value_names() {
            for name in names {
                if needs_space {
                    usage.push(' ');
                }

                usage.push_str(name);
                needs_space = true;
            }
        }

        if !pos.is_required_set() {
            usage.push(']');
            needs_space = true;
        }
    }

    if usage.is_empty() {
        name.to_owned()
    } else {
        std::format!("{name} {usage}")
    }
}

fn brush_help_styles() -> clap::builder::Styles {
    styling::Styles::styled()
        .header(
            styling::AnsiColor::Yellow.on_default()
                | styling::Effects::BOLD
                | styling::Effects::UNDERLINE,
        )
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Magenta.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brush_core::builtins::ParseArgs;
    use clap::Parser;

    fn parse_as<T: ParseArgs>(name: &str, words: &[&str]) -> Result<T, UsageError> {
        T::parse(Invocation::new(name, words.iter().copied()))
    }

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct TestArgs {
        #[arg(short = 'n')]
        flag: bool,

        #[arg(short = 'd')]
        value: Option<String>,

        operands: Vec<String>,
    }

    crate::impl_parse_args!(TestArgs);

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct PlusArgs {
        #[arg(short = 'x')]
        enable: bool,

        #[arg(long = "+x")]
        disable: bool,

        #[arg(short = 'o', long = "+o", num_args = 0..=1)]
        option: Option<String>,

        operands: Vec<String>,
    }

    crate::impl_parse_args!(PlusArgs);

    /// Removes the escape codes painting adds.
    fn unpainted(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                chars.by_ref().find(|&c| c == 'm');
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn terminal_help_is_the_plain_help_painted() {
        let plain = <TestArgs as ParseArgs>::help("t");
        let style = TerminalStyle::new(Help::DETAILS_WIDTH, true);
        let painted = <TestArgs as ParseArgs>::terminal_help("t", &style).unwrap_or_default();
        assert_ne!(painted, plain);
        assert_eq!(unpainted(&painted.synopsis), plain.synopsis);
        assert_eq!(painted.description, plain.description);
        assert_eq!(unpainted(&painted.details), plain.details);

        // With no color, it's the plain help.
        let style = TerminalStyle::new(Help::DETAILS_WIDTH, false);
        assert_eq!(
            <TestArgs as ParseArgs>::terminal_help("t", &style),
            Some(plain)
        );
    }

    #[test]
    #[allow(clippy::panic)]
    fn parses_flags_and_values() {
        let parsed = parse_as::<TestArgs>("echo", &["-n", "-d", ":", "a"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.value.as_deref(), Some(":"));
        assert_eq!(parsed.operands, ["a"]);
    }

    #[test]
    fn reports_usage_errors() {
        let err = parse_as::<TestArgs>("echo", &["--frobnicate"]).unwrap_err();
        assert!(!err.is_help_request());
    }

    #[test]
    fn a_missing_option_value_is_reported_as_bash_does() {
        let err = parse_as::<TestArgs>("printf", &["-d"]).unwrap_err();
        assert_eq!(err, UsageError::requires_argument("-d"));
    }

    #[test]
    fn unknown_plus_options_are_named_as_given() {
        let err = parse_as::<PlusArgs>("declare", &["+Z"]).unwrap_err();
        assert_eq!(err, UsageError::invalid_option("+Z"));
    }

    #[test]
    fn unknown_options_are_reported_as_bash_does() {
        // The shell adds the builtin's name and the usage line; the message
        // names only the option.
        let err = parse_as::<TestArgs>("shopt", &["--frobnicate"]).unwrap_err();
        assert_eq!(err, UsageError::invalid_option("--frobnicate"));
    }

    #[test]
    #[allow(clippy::panic)]
    fn renames_plus_options() {
        let parsed = parse_as::<PlusArgs>("declare", &["+x"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert!(parsed.disable);
        assert!(!parsed.enable);
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_words_pass_through_for_commands_without_plus_options() {
        let parsed = parse_as::<TestArgs>("echo", &["+x"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.operands, ["+x"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_operands_survive_renaming() {
        // Renaming must stop once operands begin, or the '+y' here would be
        // mangled into a '--+y' option that clap then rejects.
        let parsed = parse_as::<PlusArgs>("set", &["+x", "operand", "+y"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert!(parsed.disable);
        assert_eq!(parsed.operands, ["operand", "+y"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn option_values_are_not_mistaken_for_operands() {
        // 'later' is the value of '+o', so '+x' after it is still an option.
        let parsed = parse_as::<PlusArgs>("set", &["+o", "later", "+x"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.option.as_deref(), Some("later"));
        assert!(parsed.disable);
    }

    /// Like `compopt`: value-taking options whose arity clap infers from the
    /// field type rather than from an explicit `num_args`.
    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct InferredArityArgs {
        #[arg(short = 'o')]
        enabled: Vec<String>,

        #[arg(long = "+o")]
        disabled: Vec<String>,

        operands: Vec<String>,
    }

    crate::impl_parse_args!(InferredArityArgs);

    #[test]
    #[allow(clippy::panic)]
    fn inferred_option_values_are_not_mistaken_for_operands() {
        // 'nospace' is the value of '-o', so '+o' after it is still an option.
        let parsed =
            parse_as::<InferredArityArgs>("compopt", &["-o", "nospace", "+o", "filenames", "cmd"])
                .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.enabled, ["nospace"]);
        assert_eq!(parsed.disabled, ["filenames"]);
        assert_eq!(parsed.operands, ["cmd"]);
    }

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct DoubleDashArgs {
        #[arg(short = 'n')]
        flag: bool,

        #[arg(short = 's', allow_hyphen_values = true)]
        separator: Option<String>,

        operands: Vec<String>,
    }

    crate::impl_parse_args!(DoubleDashArgs, keep_double_dash = operands);

    #[test]
    #[allow(clippy::panic)]
    fn a_double_dash_that_ends_the_options_is_kept_with_what_follows() {
        let parsed = parse_as::<DoubleDashArgs>("echo", &["-n", "--", "-n", "b"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.operands, ["--", "-n", "b"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn the_first_double_dash_is_kept_wherever_it_is() {
        let parsed = parse_as::<DoubleDashArgs>("echo", &["-n", "a", "--", "b"])
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.operands, ["a", "--", "b"]);
    }

    #[test]
    fn keep_double_dash_still_reports_errors_before_the_separator() {
        let err = parse_as::<DoubleDashArgs>("echo", &["-z", "--", "a"]).unwrap_err();
        assert_eq!(err, UsageError::invalid_option("-z"));
    }

    /// Has subcommands, one of which must be given.
    #[derive(Parser, Debug)]
    struct WithSubcommands {
        #[clap(subcommand)]
        command: Subcommands,
    }

    #[derive(clap::Subcommand, Debug)]
    enum Subcommands {
        /// Does the first thing.
        First {
            /// Its value.
            value: String,
        },
    }

    crate::impl_parse_args!(WithSubcommands);

    #[test]
    fn help_for_a_subcommand_is_the_subcommand_s() {
        let err = parse_as::<WithSubcommands>("cmd", &["first", "--help"]).unwrap_err();
        assert!(!err.is_help_request());
        assert!(err.to_string().contains("Does the first thing"), "{err}");
    }

    #[test]
    fn a_missing_subcommand_asks_for_help() {
        let err = parse_as::<WithSubcommands>("cmd", &[]).unwrap_err();
        assert!(err.is_help_request());
    }

    #[test]
    fn a_missing_argument_is_named() {
        let err = parse_as::<WithSubcommands>("cmd", &["first"]).unwrap_err();
        assert!(err.to_string().contains("<VALUE>"), "{err}");
    }

    #[test]
    fn help_requests_are_distinguished() {
        #[derive(Parser, Debug)]
        struct HelpArgs {
            #[arg(short = 'n')]
            flag: bool,
        }

        crate::impl_parse_args!(HelpArgs);

        let err = parse_as::<HelpArgs>("cmd", &["--help"]).unwrap_err();
        assert!(err.is_help_request());
    }

    #[test]
    fn synopsis_is_unframed_and_names_the_builtin() {
        // The shell adds the `name: ` prefix and newline; the adapter must not.
        let synopsis = <TestArgs as ParseArgs>::help("mybuiltin").synopsis;
        assert!(synopsis.starts_with("mybuiltin "), "got: {synopsis}");
        assert!(
            !synopsis.contains(':') && !synopsis.ends_with('\n'),
            "got: {synopsis}"
        );
    }
}
