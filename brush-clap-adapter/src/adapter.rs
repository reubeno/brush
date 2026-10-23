//! Parses builtin arguments with clap and renders their help: what
//! [`crate::impl_parse_args`] expands to.

use brush_core::CommandArg;
use brush_core::builtins::{ContentOptions, ContentType, Invocation, UsageError};
use clap::builder::styling;

/// Converts a clap failure into a usage error, whose message is clap's
/// rendering of it.
///
/// A request for help becomes a [help request](UsageError::help), answered
/// with the help clap rendered: the command's, or the subcommand's that was
/// asked about.
pub fn to_usage_error(err: &clap::Error) -> UsageError {
    if err.kind() == clap::error::ErrorKind::DisplayHelp {
        let help = err.render();
        UsageError::help(move |options| Ok(render(&help, options)))
    } else {
        UsageError::new(err.to_string())
    }
}

/// Parses a builtin's arguments with clap.
///
/// # Arguments
///
/// * `invocation` - The invocation; its name occupies clap's bin-name slot.
///
/// # Errors
///
/// Returns a [`UsageError`] if clap rejects the arguments, or they ask for
/// help (a [help request](UsageError::help)).
pub fn parse<T: clap::Parser>(invocation: Invocation<'_>) -> Result<T, UsageError> {
    try_parse_from::<T>(argv(invocation)).map_err(|e| to_usage_error(&e))
}

/// Like [`parse`], for a builtin that takes options with a leading `+` as
/// well as `-`: each `+x` reaches clap as `--+x`. Arguments after `--` aren't
/// options, so they're left alone.
///
/// # Arguments
///
/// * `invocation` - The invocation.
///
/// # Errors
///
/// Returns the same errors as [`parse`].
pub fn parse_with_plus_options<T: clap::Parser>(
    invocation: Invocation<'_>,
) -> Result<T, UsageError> {
    try_parse_from::<T>(rewrite_plus_options(argv(invocation))).map_err(|e| to_usage_error(&e))
}

/// Like [`parse`], but leaves the first `--`, and every word after it,
/// unparsed, and returns them.
///
/// Exists because clap consumes `--` itself, while builtins such as `echo`
/// and `test` need to see it; see <https://github.com/clap-rs/clap/issues/5055>.
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
    try_parse_before_double_dash::<T>(argv(invocation)).map_err(|e| to_usage_error(&e))
}

/// Parses a declaration utility's arguments: its options with clap, and the
/// rest as declarations, which it returns as they arrived.
///
/// The options are the words that start with `-` or `+`, and aren't only
/// that, up to the first word that doesn't or to `--`, which is dropped.
///
/// # Arguments
///
/// * `invocation` - The invocation.
/// * `plus_options` - Whether the builtin takes options with a leading `+`;
///   see [`parse_with_plus_options`].
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the options.
pub fn parse_declarations<T: clap::Parser>(
    invocation: Invocation<'_>,
    plus_options: bool,
) -> Result<(T, Vec<CommandArg>), UsageError> {
    let mut options = vec![invocation.name().to_owned()];
    let mut declarations = vec![];

    let mut args = invocation.into_args().into_iter();
    for arg in args.by_ref() {
        match arg {
            CommandArg::String(s) if s == "--" => break,
            CommandArg::String(s) if s.len() > 1 && (s.starts_with('-') || s.starts_with('+')) => {
                options.push(s);
            }
            _ => {
                declarations.push(arg);
                break;
            }
        }
    }
    declarations.extend(args);

    let options = if plus_options {
        rewrite_plus_options(options)
    } else {
        options
    };

    let this = try_parse_from::<T>(options).map_err(|e| to_usage_error(&e))?;
    Ok((this, declarations))
}

/// Returns `T`'s clap command, with clap's `-h, --help` flag replaced by a
/// `--help` flag: `-h` isn't a request for help, but an option the builtin
/// doesn't take. A command that disables clap's flag is left alone.
///
/// The command renders its help as [`content`] does, so a help request's help
/// is the same as the help `help NAME` shows.
fn command<T: clap::CommandFactory>() -> clap::Command {
    let command = T::command()
        .styles(brush_help_styles())
        .next_line_help(false);
    if command.is_disable_help_flag_set() {
        return command;
    }
    command.disable_help_flag(true).arg(
        clap::Arg::new("help")
            .long("help")
            .action(clap::ArgAction::HelpShort)
            .global(true)
            .help("Print help"),
    )
}

/// Parses `args` (starting with the bin name) as `T`, with [`command`]'s
/// help flag.
fn try_parse_from<T: clap::Parser>(
    args: impl IntoIterator<Item = String>,
) -> Result<T, clap::Error> {
    let mut command = command::<T>();
    let mut args = args.into_iter().peekable();
    // Clap takes a bin name from a path's file name, and `.` has none.
    if let Some(name) = args.peek() {
        command.set_bin_name(name);
    }
    let mut matches = command.try_get_matches_from_mut(args)?;
    T::from_arg_matches_mut(&mut matches).map_err(|e| e.format(&mut command))
}

/// Returns the invocation's name, followed by its arguments as strings: what
/// clap parses.
fn argv(invocation: Invocation<'_>) -> impl Iterator<Item = String> {
    let name = invocation.name().to_owned();
    std::iter::once(name).chain(invocation.into_args().into_iter().map(String::from))
}

/// Renames options with a leading `+` (e.g. `+x`, `+ab`) to the per-character
/// long options (`--+x`, `--+a --+b`) that clap accepts, since clap has no
/// notion of a `+`-prefixed option. Arguments after `--` aren't options, so
/// they're left alone (e.g. a command name starting with `+`).
fn rewrite_plus_options(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let args = args.into_iter();
    let mut updated_args = Vec::with_capacity(args.size_hint().0);
    let mut options_ended = false;
    for arg in args {
        if options_ended {
            updated_args.push(arg);
        } else if let Some(plus_options) = arg.strip_prefix("+") {
            updated_args.extend(plus_options.chars().map(|c| format!("--+{c}")));
        } else {
            options_ended = arg == "--";
            updated_args.push(arg);
        }
    }
    updated_args
}

/// Parses `args` (starting with the bin name) with clap, up to the first
/// `--`, and returns that `--` and every word after it; they're empty if
/// there's no `--`.
///
/// This exists to deal with clap's limitation of treating `--` like a
/// regular value (<https://github.com/clap-rs/clap/issues/5055>).
///
/// # Arguments
///
/// * `args` - The arguments, starting with the bin name.
///
/// # Errors
///
/// Returns clap's error if it rejects the arguments before the separator.
pub fn try_parse_before_double_dash<T: clap::Parser>(
    args: impl IntoIterator<Item = String>,
) -> Result<(T, Vec<String>), clap::Error> {
    let mut args = args.into_iter();
    let mut found_double_dash = false;
    let parsed = try_parse_from::<T>(args.by_ref().take_while(|a| {
        found_double_dash = a == "--";
        !found_double_dash
    }))?;

    let rest = if found_double_dash {
        std::iter::once("--".to_owned()).chain(args).collect()
    } else {
        vec![]
    };
    Ok((parsed, rest))
}

/// Returns a clap-derived builtin's help content of the given type.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
/// * `content_type` - The type of content to retrieve.
/// * `options` - Additional options for content retrieval.
///
/// # Errors
///
/// Returns an error for a man page, which isn't implemented.
pub fn content<T: clap::Parser>(
    name: &str,
    content_type: ContentType,
    options: &ContentOptions,
) -> Result<String, brush_core::Error> {
    let mut clap_command = command::<T>();
    clap_command.set_bin_name(name);

    let s = match content_type {
        ContentType::DetailedHelp => render(&clap_command.render_help(), options),
        ContentType::ShortUsage => get_builtin_short_usage(name, &clap_command),
        ContentType::ShortDescription => get_builtin_short_description(name, &clap_command),
        ContentType::ManPage => {
            brush_core::error::unimp("man page rendering is not yet implemented")?
        }
    };

    Ok(s)
}

/// Renders clap's styled help, in color if `options` asks for it.
fn render(help: &clap::builder::StyledStr, options: &ContentOptions) -> String {
    if options.colorized {
        help.ansi().to_string()
    } else {
        help.to_string()
    }
}

fn get_builtin_short_description(name: &str, command: &clap::Command) -> String {
    let about = command
        .get_about()
        .map_or_else(String::new, |s| s.to_string());

    std::format!("{name} - {about}\n")
}

fn get_builtin_short_usage(name: &str, command: &clap::Command) -> String {
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

        for name in pos.get_value_names().into_iter().flatten() {
            if needs_space {
                usage.push(' ');
            }

            usage.push_str(name);
            needs_space = true;
        }

        if !pos.is_required_set() {
            usage.push(']');
            needs_space = true;
        }
    }

    std::format!("{name}: {name} {usage}\n")
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
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Flags {
        #[arg(short = 'x')]
        x: bool,
        #[arg(long = "+x")]
        plus_x: bool,
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    }

    #[test]
    fn plus_options_reach_clap_as_long_options_until_double_dash() {
        let flags: Flags =
            parse_with_plus_options(Invocation::new("f", ["+x", "--", "+y"])).unwrap();
        assert!(flags.plus_x && !flags.x);
        assert_eq!(flags.args, ["+y"]);
    }

    #[test]
    fn double_dash_and_what_follows_are_left_unparsed() {
        let (flags, rest): (Flags, _) =
            parse_with_double_dash(Invocation::new("f", ["-x", "a", "--", "-x"])).unwrap();
        assert!(flags.x);
        assert_eq!(flags.args, ["a"]);
        assert_eq!(rest, ["--", "-x"]);
    }

    #[test]
    fn declaration_utilities_get_their_options_parsed_and_the_rest_as_given() {
        let (flags, declarations): (Flags, _) =
            parse_declarations(Invocation::new("f", ["-x", "+x", "a=1", "-"]), true).unwrap();
        assert!(flags.x && flags.plus_x);
        let declarations: Vec<_> = declarations.into_iter().map(String::from).collect();
        assert_eq!(declarations, ["a=1", "-"]);
    }

    #[test]
    fn options_end_at_the_first_declaration() {
        let (flags, declarations): (Flags, _) =
            parse_declarations(Invocation::new("f", ["a=1", "-x", "+x"]), true).unwrap();
        assert!(!flags.x && !flags.plus_x);
        let declarations: Vec<_> = declarations.into_iter().map(String::from).collect();
        assert_eq!(declarations, ["a=1", "-x", "+x"]);
    }

    #[test]
    fn options_end_at_a_double_dash_which_is_dropped() {
        let (flags, declarations): (Flags, _) =
            parse_declarations(Invocation::new("f", ["-x", "--", "-x", "--"]), true).unwrap();
        assert!(flags.x && !flags.plus_x);
        let declarations: Vec<_> = declarations.into_iter().map(String::from).collect();
        assert_eq!(declarations, ["-x", "--"]);
    }

    #[derive(Parser, Debug)]
    struct Strict {
        #[arg(short = 'x')]
        x: bool,
    }

    #[test]
    fn a_usage_error_is_clap_s_rendering_of_the_failure() {
        let err = parse::<Strict>(Invocation::new("f", ["-z"])).unwrap_err();
        let clap_err = Strict::try_parse_from(["f", "-z"]).unwrap_err();
        assert_eq!(err.to_string(), clap_err.to_string());
    }

    #[test]
    fn a_usage_error_names_the_invoked_name() {
        let err = parse::<Strict>(Invocation::new("alias", ["-z"])).unwrap_err();
        assert!(err.to_string().contains("Usage: alias"), "{err}");
    }

    /// Splits `args` with [`try_parse_before_double_dash`], into what clap
    /// parsed and the rest.
    fn split(args: &[&str]) -> Result<(Flags, Vec<String>), clap::Error> {
        try_parse_before_double_dash(args.iter().map(|&a| a.to_owned()))
    }

    #[test]
    fn without_a_double_dash_everything_is_parsed_and_nothing_is_left() {
        let (flags, rest) = split(&["f", "-x", "a"]).unwrap();
        assert!(flags.x);
        assert_eq!(flags.args, ["a"]);
        assert_eq!(rest, Vec::<String>::new());
    }

    #[test]
    fn a_leading_double_dash_leaves_everything_unparsed() {
        let (flags, rest) = split(&["f", "--", "-x"]).unwrap();
        assert!(!flags.x && flags.args.is_empty());
        assert_eq!(rest, ["--", "-x"]);
    }

    #[test]
    fn only_the_first_double_dash_splits() {
        let (flags, rest) = split(&["f", "a", "--", "b", "--", "c"]).unwrap();
        assert_eq!(flags.args, ["a"]);
        assert_eq!(rest, ["--", "b", "--", "c"]);
    }

    #[test]
    fn a_trailing_double_dash_is_left_alone() {
        let (_, rest) = split(&["f", "-x", "--"]).unwrap();
        assert_eq!(rest, ["--"]);
    }

    #[test]
    fn only_words_before_the_double_dash_are_checked() {
        let err = try_parse_before_double_dash::<Strict>(["f", "-z", "--"].map(str::to_owned))
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);

        let (strict, rest) =
            try_parse_before_double_dash::<Strict>(["f", "--", "-z"].map(str::to_owned)).unwrap();
        assert!(!strict.x);
        assert_eq!(rest, ["--", "-z"]);
    }

    #[test]
    fn plus_options_become_one_long_option_per_letter() {
        let rewritten = rewrite_plus_options(["f", "+ab", "-c", "x", "-"].map(str::to_owned));
        assert_eq!(rewritten, ["f", "--+a", "--+b", "-c", "x", "-"]);
    }

    #[test]
    fn plus_options_after_a_double_dash_are_left_alone() {
        let rewritten =
            rewrite_plus_options(["f", "+a", "--", "+b", "--", "+c"].map(str::to_owned));
        assert_eq!(rewritten, ["f", "--+a", "--", "+b", "--", "+c"]);
    }

    #[test]
    fn declarations_are_told_apart_from_options_by_their_first_character() {
        let (flags, declarations): (Flags, _) = parse_declarations(
            Invocation::new("f", ["-x", "+x", "a", "-", "+", "--", "b"]),
            true,
        )
        .unwrap();
        assert!(flags.x && flags.plus_x);
        let declarations: Vec<_> = declarations.into_iter().map(String::from).collect();
        assert_eq!(declarations, ["a", "-", "+", "--", "b"]);
    }

    #[derive(Parser, Debug)]
    struct WithSubcommands {
        #[command(subcommand)]
        command: Sub,
    }

    #[derive(clap::Subcommand, Debug)]
    enum Sub {
        /// Lists things.
        List {
            /// How many.
            #[arg(long)]
            count: Option<u32>,
        },
    }

    #[test]
    fn a_subcommand_s_help_is_clap_s_rendering_of_it() {
        let message = help::<WithSubcommands>(&["list", "--help"], false);
        assert!(message.contains("Usage: f list"), "{message}");
        assert!(message.contains("--count"), "{message}");
    }

    #[test]
    fn a_subcommand_takes_help_but_not_h() {
        let err = parse::<WithSubcommands>(Invocation::new("f", ["list", "-h"])).unwrap_err();
        assert!(!err.is_help());
        assert!(err.to_string().contains("'-h'"), "{err}");
    }

    /// Does a thing.
    ///
    /// At length, which only the long help shows.
    #[derive(Parser, Debug)]
    struct Documented {
        /// Be loud.
        ///
        /// Louder than that, which only the long help shows.
        #[arg(short = 'l')]
        loud: bool,
    }

    /// Renders the help that `args` ask `T` for.
    fn help<T: clap::Parser + std::fmt::Debug>(args: &[&str], colorized: bool) -> String {
        let err = parse::<T>(Invocation::new("f", args.iter().copied())).unwrap_err();
        assert!(err.is_help(), "{err}");
        err.render_help(&ContentOptions { colorized })
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_help_request_s_help_is_the_detailed_help_in_color_or_not() {
        for colorized in [false, true] {
            let options = ContentOptions { colorized };
            let detailed = content::<Documented>("f", ContentType::DetailedHelp, &options).unwrap();
            assert_eq!(help::<Documented>(&["--help"], colorized), detailed);
            assert_eq!(help::<Documented>(&["-l", "--help"], colorized), detailed);
            assert_eq!(detailed.contains('\x1b'), colorized, "{detailed}");
        }
        assert!(!help::<Documented>(&["--help"], false).contains("only the long help"));
    }

    #[test]
    fn help_and_misuse_name_the_invoked_name_even_if_it_is_a_dot() {
        let err = parse::<Strict>(Invocation::new(".", ["-z"])).unwrap_err();
        assert!(err.to_string().contains("Usage: . "), "{err}");
        let err = parse::<Strict>(Invocation::new(".", ["--help"])).unwrap_err();
        assert!(err.to_string().contains("Usage: . "), "{err}");
    }

    #[test]
    fn a_subcommand_s_help_is_in_color_when_asked() {
        assert!(help::<WithSubcommands>(&["list", "--help"], true).contains('\x1b'));
        assert!(!help::<WithSubcommands>(&["list", "--help"], false).contains('\x1b'));
    }

    #[test]
    fn a_command_with_subcommands_has_its_own_detailed_help() {
        let detailed =
            content::<WithSubcommands>("f", ContentType::DetailedHelp, &ContentOptions::default())
                .unwrap();
        assert_eq!(help::<WithSubcommands>(&["--help"], false), detailed);
    }
}
