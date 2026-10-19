//! Engine-neutral contract for parsing builtin command arguments.
//!
//! A builtin turns its arguments into a typed value through
//! [`ParseArgs::parse`], given an [`Invocation`]: the name it was invoked
//! under and the arguments that followed. Parsing sees only those words, not
//! the shell's state, so a parser can be tested on words alone.
//!
//! The shell does no option handling on a builtin's behalf. It expands the
//! arguments and passes them through exactly as expanded; deciding which are
//! options, operands, or declarations is entirely up to the implementation.
//!
//! `brush-core` does not depend on any parsing engine; see [`ParseArgs`].

use crate::builtins::{ContentOptions, ContentType, DeclarationRole};
use crate::{CommandArg, error};

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
    /// `ShiftCommand::parse(Invocation::new("shift", ["2"]))`.
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

    /// Returns the arguments following the name, as expanded. Each is a
    /// [`CommandArg::String`] unless the builtin is a declaration utility (see
    /// [`DeclarationRole`]), whose assignment-shaped arguments arrive as
    /// [`CommandArg::Assignment`].
    pub fn args(&self) -> &[CommandArg] {
        &self.args
    }

    /// Takes the arguments. To treat them all as strings, convert each with
    /// [`String::from`].
    pub fn into_args(self) -> Vec<CommandArg> {
        self.args
    }
}

/// Misuse of a builtin: a problem with how it was invoked.
///
/// A parser returns one from [`ParseArgs::parse`]. The shell writes its
/// message to the builtin's standard error, as it is, and the command fails
/// with the invalid-usage status (2).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct UsageError {
    message: String,
}

impl UsageError {
    /// Misuse, described by `message`.
    ///
    /// # Arguments
    ///
    /// * `message` - What was wrong, as the shell is to print it.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// The parsing half of a builtin: how its arguments are read, and its help.
///
/// See [`super::Builtin`]. Engine adapters implement it from a derive (e.g.
/// `brush_clap_adapter::impl_parse_args!`); a builtin can also implement it by
/// hand:
///
/// ```
/// use brush_core::builtins::{ContentOptions, ContentType, Invocation, ParseArgs, UsageError};
///
/// struct ShiftCommand {
///     n: Option<i32>,
/// }
///
/// impl ParseArgs for ShiftCommand {
///     fn content(
///         name: &str,
///         content_type: ContentType,
///         _options: &ContentOptions,
///     ) -> Result<String, brush_core::Error> {
///         Ok(match content_type {
///             ContentType::ShortUsage => format!("{name}: {name} [n]\n"),
///             _ => format!("{name} - Shift positional parameters.\n"),
///         })
///     }
///
///     fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
///         let args: Vec<String> = invocation.into_args().into_iter().map(String::from).collect();
///         match args.as_slice() {
///             [] => Ok(Self { n: None }),
///             [n] => n
///                 .parse()
///                 .map(|n| Self { n: Some(n) })
///                 .map_err(|_| UsageError::new(format!("shift: {n}: numeric argument required"))),
///             _ => Err(UsageError::new("shift: too many arguments")),
///         }
///     }
/// }
///
/// let shift = ShiftCommand::parse(Invocation::new("shift", ["2"])).unwrap();
/// assert_eq!(shift.n, Some(2));
/// let err = ShiftCommand::parse(Invocation::new("shift", ["1", "2"])).err().unwrap();
/// assert_eq!(err.to_string(), "shift: too many arguments");
/// ```
pub trait ParseArgs: Sized + Send {
    /// The builtin's role in declarations; see [`DeclarationRole`].
    const DECLARATION_ROLE: DeclarationRole = DeclarationRole::None;

    /// Returns the builtin's help content of the given type, as invoked under
    /// `name`.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `content_type` - The type of content to retrieve.
    /// * `options` - Additional options for content retrieval.
    ///
    /// # Errors
    ///
    /// Returns an error if the content can't be produced.
    fn content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error>;

    /// Parses an invocation's arguments.
    ///
    /// # Arguments
    ///
    /// * `invocation` - The invocation.
    ///
    /// # Errors
    ///
    /// Returns an error if the arguments are invalid.
    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use brush_parser::ast;

    fn assignment(text_name: &str, value: &str) -> CommandArg {
        CommandArg::Assignment(ast::Assignment {
            name: ast::AssignmentName::VariableName(text_name.to_owned()),
            value: ast::AssignmentValue::Scalar(ast::Word::new(value)),
            append: false,
            loc: crate::SourceSpan::default(),
        })
    }

    #[test]
    fn an_invocation_takes_words_or_arguments() {
        let from_strs = Invocation::new("cmd", ["a", "b"]);
        let from_strings = Invocation::new("cmd", vec![String::from("a"), String::from("b")]);
        let from_args = Invocation::new("cmd", [CommandArg::from("a"), "b".into()]);
        for invocation in [from_strs, from_strings, from_args] {
            assert_eq!(invocation.name(), "cmd");
            let args: Vec<_> = invocation.args().iter().map(ToString::to_string).collect();
            assert_eq!(args, ["a", "b"]);
        }
    }

    #[test]
    fn an_invocation_may_have_no_arguments() {
        let invocation = Invocation::new("cmd", Vec::<String>::new());
        assert!(invocation.args().is_empty());
        assert!(invocation.into_args().is_empty());
    }

    #[test]
    fn an_invocation_s_name_outlives_it() {
        let invocation = Invocation::new("cmd", ["a"]);
        let name = invocation.name();
        let args = invocation.into_args();
        assert_eq!((name, args.len()), ("cmd", 1));
    }

    #[test]
    fn an_invocation_keeps_its_arguments_order_and_kind() {
        let invocation = Invocation::new("cmd", ["-a".into(), assignment("x", "1 2"), "y".into()]);
        let args = invocation.clone().into_args();
        assert!(matches!(&args[0], CommandArg::String(s) if s == "-a"));
        assert!(matches!(&args[1], CommandArg::Assignment(a) if a.to_string() == "x=1 2"));
        assert!(matches!(&args[2], CommandArg::String(s) if s == "y"));
        assert_eq!(
            invocation.args().len(),
            3,
            "a clone leaves the original whole"
        );
    }

    #[test]
    fn a_usage_error_displays_its_message_as_it_is() {
        let err = UsageError::new("cmd: -z: invalid option\ncmd: usage: cmd [-a]");
        assert_eq!(
            err.to_string(),
            "cmd: -z: invalid option\ncmd: usage: cmd [-a]"
        );
        assert_eq!(UsageError::new(String::from("x")), UsageError::new("x"));
        assert_ne!(UsageError::new("x"), UsageError::new("y"));
    }

    #[test]
    fn a_usage_error_is_an_error_with_no_source() {
        let err: Box<dyn std::error::Error> = Box::new(UsageError::new("cmd: bad"));
        assert!(err.source().is_none());
        assert_eq!(err.to_string(), "cmd: bad");
    }
}
