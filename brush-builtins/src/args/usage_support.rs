//! Shared usage-rs runtime support for engine-side builtin argument modules.

use std::ffi::{OsStr, OsString};

use brush_core::args::ArgsError;

/// Bridge between types that derive `usage::Cli` and this module's machinery.
///
/// usage's derive generates *inherent* methods rather than trait
/// implementations, so there is no upstream trait to bound on; implementations
/// forward to the generated inherent methods.
pub trait UsageArgs: Sized {
    /// Parses the given words (which do *not* include a program name).
    fn parse_argv<'v>(argv: &[&'v OsStr]) -> Result<Self, usage::argv::Error<'static, 'v>>;

    /// Returns the static spec metadata generated for this type.
    #[doc(hidden)]
    fn usage_spec() -> &'static usage::spec::Spec<'static>;

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

    /// Returns the short options that take a value.
    fn value_taking_short_options() -> &'static str {
        ""
    }

    /// Stores trailing (verbatim) arguments captured during parsing.
    fn set_trailing_args(&mut self, _args: Vec<String>) {}

    /// Parses the invocation words (including the command name at index 0).
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
            let (options, trailing) =
                split_option_section(&args, Self::value_taking_short_options(), &[]);

            let mut command = parse_words::<Self>(options)?;
            command.set_trailing_args(trailing);

            Ok(command)
        } else {
            parse_words::<Self>(args)
        }
    }
}

/// Implements [`UsageArgs`] for a type deriving `usage::Cli`.
#[macro_export]
macro_rules! impl_usage_parse {
    ($ty:ty) => {
        impl $crate::args::UsageArgs for $ty {
            fn parse_argv<'v>(
                argv: &[&'v std::ffi::OsStr],
            ) -> Result<Self, usage::argv::Error<'static, 'v>> {
                <$ty>::parse_from(argv)
            }

            #[doc(hidden)]
            fn usage_spec() -> &'static usage::spec::Spec<'static> {
                <$ty>::spec()
            }
        }
    };
}

pub(crate) fn render_parse_failure(
    spec: &usage::spec::Spec<'_>,
    argv: &[&OsStr],
    err: &usage::Error<'_, '_>,
) -> ArgsError {
    use usage::Error;

    let (message, help_request) = match err {
        Error::Help { cmd, long } => (
            usage::help::render_styled(spec, cmd, *long, usage::help::Style::auto())
                .unwrap_or_default(),
            true,
        ),
        Error::HelpAll { cmd } => (
            usage::help::render_styled(spec, cmd, true, usage::help::Style::auto())
                .unwrap_or_default(),
            true,
        ),
        Error::MissingArgsHelp { cmd } => (
            usage::help::render_styled(spec, cmd, false, usage::help::Style::auto_stderr())
                .unwrap_or_default(),
            false,
        ),
        _ => (usage::render_failure(spec, argv, err), false),
    };

    ArgsError {
        message,
        help_request,
    }
}

fn parse_words<T: UsageArgs>(words: Vec<String>) -> Result<T, ArgsError> {
    let os_args: Vec<OsString> = words.into_iter().map(Into::into).collect();
    let refs: Vec<&OsStr> = os_args.iter().map(OsString::as_os_str).collect();

    match T::parse_argv(&refs) {
        Ok(parsed) => Ok(parsed),
        Err(err) => Err(render_parse_failure(T::usage_spec(), &refs, &err)),
    }
}

/// Expands groups of plus-style options (e.g., `+vx`) into individual tokens.
pub(crate) fn expand_plus_option_groups(args: Vec<String>) -> Vec<String> {
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

fn short_group_token_count(group: &str, value_shorts: &str) -> usize {
    let char_count = group.chars().count();
    for (j, c) in group.chars().enumerate() {
        if value_shorts.contains(c) {
            return if j == char_count - 1 { 2 } else { 1 };
        }
    }

    1
}

/// Splits an argument list into the option section and trailing operands.
pub(crate) fn split_option_section(
    args: &[String],
    value_shorts: &str,
    _value_longs: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();

        if arg == "--" {
            return (args[..i].to_vec(), args[i..].to_vec());
        }

        if arg.starts_with("--") {
            i += 1;
        } else if let Some(shorts) = arg.strip_prefix('-').filter(|s| !s.starts_with('-')) {
            i += short_group_token_count(shorts, value_shorts);
        } else if let Some(plus) = arg.strip_prefix('+').filter(|p| !p.is_empty()) {
            i += short_group_token_count(plus, value_shorts);
        } else {
            return (args[..i].to_vec(), args[i..].to_vec());
        }
    }

    (args.to_vec(), Vec::new())
}

/// Renders detailed help by triggering usage's `--help` handling.
pub fn get_content<T: UsageArgs>(
    name: &str,
    content_type: &brush_core::builtins::ContentType,
    _options: &brush_core::builtins::ContentOptions,
) -> Result<String, brush_core::error::Error> {
    use brush_core::builtins::ContentType;

    match content_type {
        ContentType::DetailedHelp => {
            let help_args = [OsStr::new("--help")];
            match T::parse_argv(&help_args) {
                Err(err) => {
                    let argv: Vec<&OsStr> = help_args.iter().copied().collect();
                    Ok(render_parse_failure(T::usage_spec(), &argv, &err).message)
                }
                Ok(_) => Err(brush_core::error::ErrorKind::Unimplemented(
                    "unexpectedly parsed help request",
                )
                .into()),
            }
        }
        ContentType::ShortUsage => Ok(format!("{name}: {name} {}\n", T::synopsis())),
        ContentType::ShortDescription => Ok(format!("{name} - {}\n", T::about())),
        ContentType::ManPage => brush_core::error::unimp("man page rendering not yet implemented"),
    }
}
