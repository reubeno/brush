//! Engine-neutral contract for parsing builtin command arguments.
//!
//! A builtin turns its arguments into a typed value through [`Builtin::parse`]
//! (or, for a builtin whose arguments type is the whole builtin,
//! [`ParseArgs::parse`]), given an [`Invocation`]: the name it was invoked
//! under and the arguments that followed. Parsing sees only those words, not
//! the shell's state, so a parser can be tested, or reused (to complete a
//! command line, say), on words alone; a builtin whose arguments mean something
//! different depending on the shell's options decides that when it runs.
//! Nothing in this module names an argument-parsing engine.
//!
//! The shell does no option handling on a builtin's behalf. It expands the
//! arguments and passes them through exactly as expanded; deciding which are
//! options, operands, or declarations is entirely up to the implementation. The
//! shell does own how a problem with the arguments is *reported*: a
//! [`UsageError`] carries what went wrong, and the shell renders it and
//! attributes it to the builtin.
//!
//! `brush-core` does not depend on any parsing engine; see [`ParseArgs`].
//!
//! [`Builtin::parse`]: super::Builtin::parse

use std::fmt;

use crate::builtins::{Help, TerminalStyle};
use crate::{CommandArg, ExecutionExitCode};

/// One invocation of a builtin, as handed to its parser.
///
/// Its fields are private, so it can carry more later without breaking parsers.
#[derive(Clone, Debug)]
pub struct Invocation<'a> {
    name: &'a str,
    args: Vec<CommandArg>,
}

impl<'a> Invocation<'a> {
    /// Creates an invocation. The shell creates them; this is public so that a
    /// builtin's parser can be tested on its own:
    /// `ShiftArgs::parse(Invocation::new("shift", ["abc"]))`.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `args` - The arguments following the name: words (`&str` or `String`)
    ///   or [`CommandArg`]s.
    pub fn new(name: &'a str, args: impl IntoIterator<Item = impl Into<CommandArg>>) -> Self {
        Self {
            name,
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the name the builtin was invoked under. It is never repeated in
    /// the arguments.
    pub const fn name(&self) -> &'a str {
        self.name
    }

    /// Returns the arguments following the name, as expanded; the name itself
    /// is never among them (see [`name`](Self::name)). Each is a
    /// [`CommandArg::String`] unless the builtin is a declaration utility (see
    /// [`super::Declarations`]), whose assignment-shaped arguments arrive as
    /// [`CommandArg::Declaration`].
    pub fn args(&self) -> &[CommandArg] {
        &self.args
    }

    /// Takes the arguments.
    pub fn into_args(self) -> Vec<CommandArg> {
        self.args
    }

    /// Takes the arguments as words. A declaration renders as `name=value`, with
    /// an array value's elements run together (`x=(a b c)` for `x=(a 'b c')`),
    /// so a declaration utility should read [`args`](Self::args) instead.
    pub fn into_words(self) -> Vec<String> {
        self.args.into_iter().map(CommandArg::into_word).collect()
    }
}

/// Misuse of a builtin: a problem with how it was invoked.
///
/// A parser returns one from [`ParseArgs::parse`] or [`Builtin::parse`].
///
/// The shell prints `name: {message}` on the builtin's standard error,
/// followed, if the error says so ([`UsageError::with_usage_line`]), by the
/// usage line (`name: usage: {synopsis}`). A builtin never formats its own
/// name, a usage line, or a location into the message. For a help request, it
/// prints the builtin's full help on standard output instead. The command fails
/// with the invalid-usage status (2), as in bash.
///
/// [`Builtin::parse`]: super::Builtin::parse
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError {
    kind: UsageErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UsageErrorKind {
    Invalid { message: String, usage_line: bool },
    HelpRequested,
}

impl UsageError {
    const fn of(kind: UsageErrorKind) -> Self {
        Self { kind }
    }

    /// Misuse, described by `message` and reported without the usage line, as
    /// bash reports most misuse (e.g. `abc: numeric argument required`). Add the
    /// usage line with [`with_usage_line`](Self::with_usage_line).
    ///
    /// # Arguments
    ///
    /// * `message` - What was wrong, without the builtin's name.
    pub fn new(message: impl Into<String>) -> Self {
        Self::of(UsageErrorKind::Invalid {
            message: message.into(),
            usage_line: false,
        })
    }

    /// An unrecognized option, reported as bash does: `-z: invalid option`,
    /// followed by the usage line.
    ///
    /// # Arguments
    ///
    /// * `option` - The option as given, with its leading `-` or `+`.
    pub fn invalid_option(option: impl fmt::Display) -> Self {
        Self::new(format!("{option}: invalid option")).with_usage_line()
    }

    /// An option missing its value, reported as bash does: `-n: option requires
    /// an argument`, followed by the usage line.
    ///
    /// # Arguments
    ///
    /// * `option` - The option as given, with its leading `-` or `+`.
    pub fn requires_argument(option: impl fmt::Display) -> Self {
        Self::new(format!("{option}: option requires an argument")).with_usage_line()
    }

    /// The arguments asked for help (`--help`). The shell prints the builtin's
    /// full help on standard output, as bash does, and fails the command with
    /// status 2. The shell doesn't recognize `--help` itself: a parser that
    /// wants it returns this.
    pub const fn help_requested() -> Self {
        Self::of(UsageErrorKind::HelpRequested)
    }

    /// Returns the error with the usage line (`name: usage: {synopsis}`) after
    /// its message, as bash prints after an unknown option, say. It has no
    /// effect on a help request, which prints the full help.
    #[must_use]
    pub const fn with_usage_line(mut self) -> Self {
        if let UsageErrorKind::Invalid { usage_line, .. } = &mut self.kind {
            *usage_line = true;
        }
        self
    }

    /// Returns the exit code the command fails with: the invalid-usage
    /// status (2).
    pub const fn exit_code(&self) -> ExecutionExitCode {
        ExecutionExitCode::InvalidUsage
    }

    /// Returns whether the arguments asked for help.
    pub const fn is_help_request(&self) -> bool {
        matches!(self.kind, UsageErrorKind::HelpRequested)
    }

    /// Returns whether the shell follows the message with the usage line.
    pub(crate) const fn shows_usage_line(&self) -> bool {
        matches!(
            self.kind,
            UsageErrorKind::Invalid {
                usage_line: true,
                ..
            }
        )
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            UsageErrorKind::Invalid { message, .. } => f.write_str(message),
            UsageErrorKind::HelpRequested => f.write_str("help requested"),
        }
    }
}

impl std::error::Error for UsageError {}

/// The parsing half of a builtin whose arguments type is the whole builtin.
///
/// See [`super::StatelessBuiltin`]. Engine adapters implement it from a derive (e.g.
/// `brush_builtin_clap::impl_parse_args!`); a builtin can also implement it by
/// hand. The shell gives `--help` no meaning of its own, so a parser that
/// implements it returns [`UsageError::help_requested`]:
///
/// ```
/// use brush_core::builtins::{UsageError, Help, Invocation, ParseArgs};
///
/// struct ShiftArgs {
///     n: Option<i32>,
/// }
///
/// impl ParseArgs for ShiftArgs {
///     fn help(name: &str) -> Help {
///         Help::new(format!("{name} [n]"), "Shift positional parameters.")
///     }
///
///     fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
///         match invocation.into_words().as_slice() {
///             [] => Ok(Self { n: None }),
///             [help] if help == "--help" => Err(UsageError::help_requested()),
///             [n] => n
///                 .parse()
///                 .map(|n| Self { n: Some(n) })
///                 .map_err(|_| UsageError::new(format!("{n}: numeric argument required"))),
///             _ => Err(UsageError::new("too many arguments")),
///         }
///     }
/// }
/// ```
pub trait ParseArgs: Sized + Send {
    /// Returns the builtin's help, as invoked under `name`, in plain text with
    /// its details wrapped to [`Help::DETAILS_WIDTH`]. It is also the source of
    /// the usage line the shell prints with a usage error, and of what
    /// `--help` prints where the output isn't a terminal.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    fn help(name: &str) -> Help;

    /// Returns the builtin's help rendered for a terminal of `style`, as
    /// invoked under `name`: wrapped to its width and painted for its color
    /// depth. `None` has the shell print [`help`](Self::help) as it is. It's
    /// the same text as `help`'s, wrapped to a different width and with escape
    /// codes: the shell uses `help` wherever it needs plain text (a usage line,
    /// say). By default, `None`.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `style` - The terminal's width and color depth.
    fn terminal_help(name: &str, style: &TerminalStyle) -> Option<Help> {
        let _ = (name, style);
        None
    }

    /// Parses an invocation's arguments.
    ///
    /// # Arguments
    ///
    /// * `invocation` - The invocation.
    ///
    /// # Errors
    ///
    /// Returns an error if the arguments are invalid, or if they ask for help.
    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError>;
}
