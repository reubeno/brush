//! Binds builtins that use clap to the engine-neutral contracts in
//! [`brush_core::builtins::args`] and [`brush_core::builtins::HelpContent`].
//!
//! `brush-core` does not depend on clap. A builtin opts in by invoking
//! [`crate::clap_builtin`]; a builtin that uses a different engine implements
//! those two traits itself and never touches this module.

use clap::builder::styling;

use brush_core::CommandArg;
use brush_core::builtins::{ArgsError, ContentOptions};
use brush_core::error;

use crate::args::{into_words, split_leading_options};

/// Classifies a clap failure as a help request or a usage error.
fn to_args_error(err: &clap::Error) -> ArgsError {
    let message = err.to_string();
    match err.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            ArgsError::HelpRequested(message)
        }
        _ => ArgsError::Usage(message),
    }
}

/// Parses a builtin's arguments with clap. Implements
/// [`brush_core::builtins::FromArgs`] for clap-derived types; see
/// [`crate::clap_builtin`].
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under; occupies clap's bin-name
///   slot so that usage and error text name the builtin.
/// * `args` - The arguments following `name`.
///
/// # Errors
///
/// Returns [`ArgsError::HelpRequested`] if the arguments asked for help or
/// version information, and [`ArgsError::Usage`] for any other parse failure.
pub fn parse<T: clap::Parser>(name: &str, args: Vec<CommandArg>) -> Result<T, ArgsError> {
    parse_words(name, &into_words(args))
}

/// Like [`parse`], but leaves the first `--` and everything after it unparsed.
///
/// Returns the separator and every word after it, or an empty remainder when no
/// separator is present. See [`crate::clap_builtin`]'s `trailing_args` option.
///
/// Exists because clap consumes `--` itself, while builtins such as `echo` and
/// `test` need to see it; see <https://github.com/clap-rs/clap/issues/5055>.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
/// * `args` - The arguments following `name`.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the words before the separator.
pub fn parse_with_trailing<T: clap::Parser>(
    name: &str,
    args: Vec<CommandArg>,
) -> Result<(T, Vec<String>), ArgsError> {
    let mut words = into_words(args);
    let Some(separator) = words.iter().position(|w| w == "--") else {
        return Ok((parse_words(name, &words)?, Vec::new()));
    };

    let trailing = words.split_off(separator);
    Ok((parse_words(name, &words)?, trailing))
}

/// Like [`parse`], but parses only the leading options with clap.
///
/// Returns the operands after them untouched, so declarations keep their
/// assignment form. Options are the leading words that begin with `-` or `+`;
/// the first other word ends them, as does `--`, which is dropped. See
/// [`crate::clap_builtin`]'s `declarations` option.
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
/// * `args` - The arguments following `name`.
///
/// # Errors
///
/// Returns the same errors as [`parse`], for the leading options.
pub fn parse_with_declarations<T: clap::Parser>(
    name: &str,
    args: Vec<CommandArg>,
) -> Result<(T, Vec<CommandArg>), ArgsError> {
    let (options, operands) = split_leading_options(args);
    Ok((parse_words(name, &options)?, operands))
}

fn parse_words<T: clap::Parser>(name: &str, words: &[String]) -> Result<T, ArgsError> {
    let invocation = std::iter::once(name.to_owned());
    match rewrite_plus_options::<T, String>(words) {
        Some(renamed) => T::try_parse_from(invocation.chain(renamed)),
        None => T::try_parse_from(invocation.chain(words.iter().cloned())),
    }
    .map_err(|e| to_args_error(&e))
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

    let command = T::command();
    if !command
        .get_arguments()
        .any(|a| a.get_long().is_some_and(|l| l.starts_with('+')))
    {
        return None;
    }

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

        expecting_value = option_takes_value(&command, word);

        if let Some(plus_options) = word.strip_prefix('+') {
            renamed.extend(plus_options.chars().map(|c| format!("--+{c}")));
        } else {
            renamed.push(word.to_owned());
        }
    }

    Some(renamed)
}

/// Returns whether the given option word consumes the following word as its
/// value. Only the last option in a bundle (e.g. the `o` in `-ao` or `+ao`) can
/// do so.
fn option_takes_value(command: &clap::Command, word: &str) -> bool {
    let arg = if let Some(plus_options) = word.strip_prefix('+') {
        plus_options
            .chars()
            .last()
            .and_then(|c| find_long(command, format!("+{c}").as_str()))
    } else if let Some(long) = word.strip_prefix("--") {
        find_long(command, long)
    } else if let Some(shorts) = word.strip_prefix('-') {
        shorts
            .chars()
            .last()
            .and_then(|c| command.get_arguments().find(|a| a.get_short() == Some(c)))
    } else {
        None
    };

    arg.is_some_and(|a| a.get_num_args().is_some_and(|n| n.takes_values()))
}

fn find_long<'a>(command: &'a clap::Command, name: &str) -> Option<&'a clap::Arg> {
    command.get_arguments().find(|a| a.get_long() == Some(name))
}

/// Builds the synopsis for a clap-derived builtin from its declared options
/// and positionals. Implements [`brush_core::builtins::HelpContent::synopsis`];
/// see [`crate::clap_builtin`].
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
pub fn synopsis<T: clap::Parser>(name: &str) -> String {
    short_usage(name, &help_command::<T>(name))
}

/// Returns a clap-derived builtin's `about` text. Implements
/// [`brush_core::builtins::HelpContent::description`]; see
/// [`crate::clap_builtin`].
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
pub fn description<T: clap::Parser>(name: &str) -> String {
    help_command::<T>(name)
        .get_about()
        .map_or_else(String::new, |s| s.to_string())
}

/// Renders a clap-derived builtin's full help with clap. Implements
/// [`brush_core::builtins::HelpContent::detailed_help`]; see
/// [`crate::clap_builtin`].
///
/// # Arguments
///
/// * `name` - The name the builtin was invoked under.
/// * `options` - Options controlling how the content is rendered.
///
/// # Errors
///
/// Never fails; the signature matches the trait.
pub fn detailed_help<T: clap::Parser>(
    name: &str,
    options: &ContentOptions,
) -> Result<String, error::Error> {
    let rendered = help_command::<T>(name).render_help();
    Ok(if options.colorized {
        rendered.ansi().to_string()
    } else {
        rendered.to_string()
    })
}

fn help_command<T: clap::Parser>(name: &str) -> clap::Command {
    let mut command = T::command()
        .styles(brush_help_styles())
        .next_line_help(false);
    command.set_bin_name(name);
    command
}

/// Declares that a builtin uses clap, wiring up its argument parsing and its
/// help content. Expects the type to derive [`clap::Parser`].
///
/// # Forms
///
/// Exactly one of these forms is used per type; the options are mutually
/// exclusive, and any other option is a compile error naming the accepted
/// forms.
///
/// | Form | Parsing |
/// |---|---|
/// | `clap_builtin!(Type)` | Every argument is flattened to a string and parsed by clap. |
/// | `clap_builtin!(Type, trailing_args = field)` | As above, but the first `--` and every word after it are taken verbatim and appended to `field` (a `Vec<String>`) instead of letting clap consume the separator. For `echo`, `test`, `set`, `getopts`. |
/// | `clap_builtin!(Type, declarations = field)` | Only the leading options are parsed by clap; the operands after them are stored in `field` (a `Vec<CommandArg>`, marked `#[clap(skip)]`) with assignments intact, and [`FromArgs::TAKES_DECLARATIONS`](brush_core::builtins::FromArgs::TAKES_DECLARATIONS) is set. For `declare`, `export`, and similar. |
///
/// A builtin that parses nothing at all does not need clap; see
/// [`crate::verbatim_builtin`].
///
/// # Examples
///
/// ```
/// #[derive(clap::Parser)]
/// struct GreetCommand {
///     names: Vec<String>,
/// }
///
/// brush_builtin_utils::clap_builtin!(GreetCommand);
///
/// #[derive(clap::Parser)]
/// struct EchoCommand {
///     #[arg(allow_hyphen_values = true)]
///     args: Vec<String>,
/// }
///
/// brush_builtin_utils::clap_builtin!(EchoCommand, trailing_args = args);
///
/// #[derive(clap::Parser)]
/// struct ExportCommand {
///     #[arg(short = 'n')]
///     unexport: bool,
///
///     #[clap(skip)]
///     declarations: Vec<brush_core::CommandArg>,
/// }
///
/// brush_builtin_utils::clap_builtin!(ExportCommand, declarations = declarations);
/// ```
///
/// Misuse is rejected with a message naming the accepted forms:
///
/// ```compile_fail
/// # #[derive(clap::Parser)]
/// # struct T { a: Vec<String>, #[clap(skip)] b: Vec<brush_core::CommandArg> }
/// brush_builtin_utils::clap_builtin!(T, trailing_args = a, declarations = b);
/// ```
#[macro_export]
macro_rules! clap_builtin {
    ($t:ty $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                $crate::clap_adapter::parse::<Self>(name, args)
            }
        }

        $crate::__clap_builtin_help!($t);
    };

    ($t:ty, trailing_args = $field:ident $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                let (mut this, trailing) =
                    $crate::clap_adapter::parse_with_trailing::<Self>(name, args)?;
                this.$field.extend(trailing);
                Ok(this)
            }
        }

        $crate::__clap_builtin_help!($t);
    };

    ($t:ty, declarations = $field:ident $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            const TAKES_DECLARATIONS: bool = true;

            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                let (mut this, declarations) =
                    $crate::clap_adapter::parse_with_declarations::<Self>(name, args)?;
                this.$field = declarations;
                Ok(this)
            }
        }

        $crate::__clap_builtin_help!($t);
    };

    // Everything below rejects misuse with a message naming the accepted forms.
    ($t:ty, trailing_args = $a:ident, declarations = $($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: `trailing_args` and `declarations` are mutually exclusive; \
             a builtin is parsed in one mode. Accepted forms: `clap_builtin!(Type)`, \
             `clap_builtin!(Type, trailing_args = field)`, \
             `clap_builtin!(Type, declarations = field)`"
        );
    };
    ($t:ty, declarations = $a:ident, trailing_args = $($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: `trailing_args` and `declarations` are mutually exclusive; \
             a builtin is parsed in one mode. Accepted forms: `clap_builtin!(Type)`, \
             `clap_builtin!(Type, trailing_args = field)`, \
             `clap_builtin!(Type, declarations = field)`"
        );
    };
    ($t:ty, raw_args = $($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: `raw_args` was removed; a builtin that stores its arguments \
             verbatim does not need clap. Use `verbatim_builtin!(Type, args = field, \
             synopsis = ..., description = ...)` instead"
        );
    };
    ($t:ty, trailing_args = $($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: `trailing_args` takes a field name, as in \
             `clap_builtin!(Type, trailing_args = field)`, where `field` is a `Vec<String>`"
        );
    };
    ($t:ty, declarations = $($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: `declarations` takes a field name, as in \
             `clap_builtin!(Type, declarations = field)`, where `field` is a \
             `Vec<brush_core::CommandArg>` marked `#[clap(skip)]`"
        );
    };
    ($t:ty, $option:ident = $($rest:tt)*) => {
        ::std::compile_error!(::std::concat!(
            "clap_builtin!: unknown option `",
            ::std::stringify!($option),
            "`. Accepted forms: `clap_builtin!(Type)`, \
             `clap_builtin!(Type, trailing_args = field)`, \
             `clap_builtin!(Type, declarations = field)`"
        ));
    };
    ($($rest:tt)*) => {
        ::std::compile_error!(
            "clap_builtin!: expected a type that derives `clap::Parser`, optionally followed \
             by one option. Accepted forms: `clap_builtin!(Type)`, \
             `clap_builtin!(Type, trailing_args = field)`, \
             `clap_builtin!(Type, declarations = field)`"
        );
    };
}

/// Implements [`HelpContent`](brush_core::builtins::HelpContent) for a
/// clap-derived type. Internal to [`clap_builtin!`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __clap_builtin_help {
    ($t:ty) => {
        impl $crate::__brush_core::builtins::HelpContent for $t {
            fn synopsis(name: &str) -> ::std::string::String {
                $crate::clap_adapter::synopsis::<Self>(name)
            }

            fn description(name: &str) -> ::std::string::String {
                $crate::clap_adapter::description::<Self>(name)
            }

            fn detailed_help(
                name: &str,
                options: &$crate::__brush_core::builtins::ContentOptions,
            ) -> ::std::result::Result<::std::string::String, $crate::__brush_core::Error> {
                $crate::clap_adapter::detailed_help::<Self>(name, options)
            }
        }
    };
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
    use brush_core::builtins::{FromArgs, HelpContent};
    use clap::Parser;

    fn strings(words: &[&str]) -> Vec<CommandArg> {
        words
            .iter()
            .map(|w| CommandArg::String((*w).to_owned()))
            .collect()
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

    crate::clap_builtin!(TestArgs);

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

    crate::clap_builtin!(PlusArgs);

    #[test]
    #[allow(clippy::panic)]
    fn parses_flags_and_values() {
        let parsed = TestArgs::from_args("echo", strings(&["-n", "-d", ":", "a"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.value.as_deref(), Some(":"));
        assert_eq!(parsed.operands, ["a"]);
    }

    #[test]
    fn reports_usage_errors() {
        let err = TestArgs::from_args("echo", strings(&["--frobnicate"])).unwrap_err();
        assert!(matches!(err, ArgsError::Usage(_)));
    }

    #[test]
    fn usage_errors_name_the_builtin() {
        // The invoked name occupies clap's bin-name slot, so it appears in the
        // rendered message rather than the type's name.
        let err = TestArgs::from_args("shopt", strings(&["--frobnicate"])).unwrap_err();
        assert!(err.to_string().contains("shopt"), "got: {err}");
    }

    #[test]
    #[allow(clippy::panic)]
    fn renames_plus_options() {
        let parsed = PlusArgs::from_args("declare", strings(&["+x"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert!(parsed.disable);
        assert!(!parsed.enable);
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_words_pass_through_for_commands_without_plus_options() {
        let parsed = TestArgs::from_args("echo", strings(&["+x"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.operands, ["+x"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn plus_operands_survive_renaming() {
        // Renaming must stop once operands begin, or the '+y' here would be
        // mangled into a '--+y' option that clap then rejects.
        let parsed = PlusArgs::from_args("set", strings(&["+x", "operand", "+y"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert!(parsed.disable);
        assert_eq!(parsed.operands, ["operand", "+y"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn option_values_are_not_mistaken_for_operands() {
        // 'later' is the value of '+o', so '+x' after it is still an option.
        let parsed = PlusArgs::from_args("set", strings(&["+o", "later", "+x"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));
        assert_eq!(parsed.option.as_deref(), Some("later"));
        assert!(parsed.disable);
    }

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct TrailingArgs {
        #[arg(short = 'n')]
        flag: bool,

        operands: Vec<String>,
    }

    crate::clap_builtin!(TrailingArgs, trailing_args = operands);

    #[test]
    #[allow(clippy::panic)]
    fn trailing_args_keep_separator_and_everything_after_it() {
        let parsed = TrailingArgs::from_args("echo", strings(&["-n", "a", "--", "-n", "b"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.operands, ["a", "--", "-n", "b"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn trailing_args_without_separator_parse_normally() {
        let parsed = TrailingArgs::from_args("echo", strings(&["-n", "a"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.operands, ["a"]);
    }

    #[test]
    fn trailing_args_still_report_errors_before_the_separator() {
        let err = TrailingArgs::from_args("echo", strings(&["-z", "--", "a"])).unwrap_err();
        assert!(matches!(err, ArgsError::Usage(_)));
    }

    #[test]
    fn help_requests_are_distinguished() {
        #[derive(Parser, Debug)]
        struct HelpArgs {
            #[arg(short = 'n')]
            flag: bool,
        }

        crate::clap_builtin!(HelpArgs);

        let err = HelpArgs::from_args("cmd", strings(&["--help"])).unwrap_err();
        assert!(matches!(err, ArgsError::HelpRequested(_)));
    }

    #[test]
    fn synopsis_is_unframed_and_names_the_builtin() {
        // The shell adds the `name: ` prefix and newline; the adapter must not.
        let synopsis = <TestArgs as HelpContent>::synopsis("mybuiltin");
        assert!(synopsis.starts_with("mybuiltin "), "got: {synopsis}");
        assert!(
            !synopsis.contains(':') && !synopsis.ends_with('\n'),
            "got: {synopsis}"
        );
    }

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct DeclArgs {
        #[arg(short = 'x')]
        export: bool,

        #[arg(long = "+x")]
        unexport: bool,

        #[clap(skip)]
        declarations: Vec<CommandArg>,
    }

    crate::clap_builtin!(DeclArgs, declarations = declarations);

    #[test]
    #[allow(clippy::panic)]
    fn declarations_parse_only_leading_options() {
        // `-x` after an operand is an operand, as in bash.
        let parsed = DeclArgs::from_args("declare", strings(&["+x", "a", "-x"]))
            .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.unexport);
        assert!(!parsed.export);
        assert_eq!(crate::args::into_words(parsed.declarations), ["a", "-x"]);
        const { assert!(<DeclArgs as FromArgs>::TAKES_DECLARATIONS) };
    }
}
