//! The contract between the shell and its builtins.
//!
//! A builtin is a value implementing [`Builtin`]. It describes itself (its
//! [`Builtin::help`] and how it relates to [`Declarations`]), turns an
//! invocation's arguments into a typed value ([`Builtin::parse`]), and runs
//! ([`Builtin::execute`]). Because the value *is* the builtin, it can hold
//! configuration or state, and one type can be registered under several names,
//! each configured differently.
//!
//! Most builtins need no state of their own: their arguments type is the whole
//! builtin. They implement [`StatelessBuiltin`], whose parsing half,
//! [`ParseArgs`], an engine adapter can implement from a derive, and register
//! with [`Registration::stateless`].
//!
//! The shell owns everything around a builtin: expanding its arguments,
//! rendering its help and its usage errors, and printing and attributing the
//! failures it returns. Builtins run other builtins through the shell too
//! ([`crate::ExecutionContext::run_builtin`]).
//!
//! # Failure
//!
//! * For a problem with the arguments, return a [`UsageError`] from `parse`.
//!   The shell prints it, attributed to the builtin and with its usage line if
//!   the error asks for one, and the command fails with the error's exit code.
//! * For an ordinary failure, in the builtin's own words, return a [`Failure`]
//!   from `execute`. It's made from a message, or from an error of the
//!   builtin's own type, which it keeps for whatever reports it.
//! * For anything else, return the error from `execute`, with `?`. The shell
//!   prints it, attributed to the builtin (`name: message`), and the command
//!   fails with the error's exit code. A broken pipe isn't printed. For a
//!   special builtin ([`is_special`]) in POSIX mode, a returned error ends a
//!   non-interactive shell.
//!
//! A builtin with an error type of its own converts it into a
//! [`crate::Error`], so that `?` works in `execute`: a shell error it wraps
//! passes through as it is, and the rest become [`Failure`]s, each with the
//! exit code it fails with. [`Failure`]'s docs show one.

use futures::future::BoxFuture;
use std::io::Write as _;
use std::marker::PhantomData;
use std::sync::Arc;

mod args;
mod failure;
mod help;

use crate::{CommandArg, commands, error, extensions, results, variables};
pub use args::{Invocation, ParseArgs, UsageError};
pub use failure::Failure;
pub(crate) use help::wants_color;
pub use help::{Help, HelpForm, TerminalStyle};

/// A declaration operand: `name`, `name=value`, `name[index]+=value`, or
/// `name=(...)`.
///
/// A declaration builtin receives one in a [`CommandArg::Declaration`], or
/// reads one from a plain word with [`Declaration::from_word`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Declaration {
    /// The variable's name.
    pub name: String,
    /// The array subscript, for `name[index]=value`, as written (expanded if
    /// the shell expanded the declaration). It isn't evaluated or validated;
    /// the shell's assignment code interprets it when assigning.
    pub index: Option<String>,
    /// Whether the value is appended (`+=`).
    pub append: bool,
    /// The value, already expanded, if one was given.
    pub value: Option<variables::ShellValueLiteral>,
}

impl Declaration {
    /// Returns a declaration of `name` with no value.
    ///
    /// # Arguments
    ///
    /// * `name` - The variable's name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            index: None,
            append: false,
            value: None,
        }
    }

    /// Reads a word as a declaration operand, as a declaration builtin reads
    /// one the shell didn't expand as an assignment: a quoted word
    /// (`declare 'x=1'`), one that came from an expansion (`export "$a"`), or
    /// one `command` or `builtin` passed on.
    ///
    /// As in bash, the name is what precedes the first `[`, `+=`, or `=`; a
    /// subscript follows in brackets, up to the `]` that closes it (so it may
    /// contain `=`, as in `m[a=b]=1`); then `=` or `+=`, and the value, taken
    /// as it is: `x=(1 2)` in a word is the string `(1 2)`. A word of any
    /// other shape is all name. The name isn't validated; that's the builtin's
    /// job.
    ///
    /// # Arguments
    ///
    /// * `word` - The word.
    pub fn from_word(word: &str) -> Self {
        let (name, rest) = word.split_at(word.find(['[', '+', '=']).unwrap_or(word.len()));

        let (index, rest) = match rest.strip_prefix('[') {
            Some(subscript) => match split_subscript(subscript) {
                Some((index, rest)) => (Some(index), rest),
                None => return Self::new(word),
            },
            None => (None, rest),
        };

        let (append, value) = if rest.is_empty() {
            (false, None)
        } else if let Some(value) = rest.strip_prefix("+=") {
            (true, Some(value))
        } else if let Some(value) = rest.strip_prefix('=') {
            (false, Some(value))
        } else {
            return Self::new(word);
        };

        Self {
            name: name.to_owned(),
            index: index.map(str::to_owned),
            append,
            value: value.map(|value| variables::ShellValueLiteral::Scalar(value.to_owned())),
        }
    }
}

/// Splits `text`, which follows an opening `[`, at the `]` that closes it, into
/// the subscript and what follows the `]`.
fn split_subscript(text: &str) -> Option<(&str, &str)> {
    let mut depth = 1_usize;
    let close = text.find(|c| {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        depth == 0
    })?;
    let (subscript, rest) = text.split_at(close);
    Some((subscript, rest.strip_prefix(']')?))
}

/// Renders the declaration as the word it came from (`name[index]+=value`),
/// with the value as expanded, unquoted.
impl std::fmt::Display for Declaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        if let Some(index) = &self.index {
            write!(f, "[{index}]")?;
        }
        let Some(value) = &self.value else {
            return Ok(());
        };
        f.write_str(if self.append { "+=" } else { "=" })?;
        match value {
            variables::ShellValueLiteral::Scalar(value) => f.write_str(value),
            variables::ShellValueLiteral::Array(elements) => {
                f.write_str("(")?;
                for (i, (key, value)) in elements.0.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    if let Some(key) = key {
                        write!(f, "[{key}]=")?;
                    }
                    f.write_str(value)?;
                }
                f.write_str(")")
            }
        }
    }
}

/// How a builtin relates to declarations.
///
/// The shell reads this *before* expanding a command's arguments, because it
/// changes how assignment-shaped words expand: for a declaration utility, a word
/// like `x=$v` expands as an assignment (no field splitting or globbing) and
/// arrives as [`CommandArg::Declaration`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Declarations {
    /// Assignment-shaped words are ordinary words.
    #[default]
    No,
    /// A declaration utility (`declare`, `export`, `local`, `readonly`,
    /// `typeset`).
    Yes,
}

/// A builtin.
///
/// The implementing value is the builtin, so it can hold configuration or
/// state; [`Builtin::Args`] is what one invocation's arguments parse to.
/// Builtins with no state of their own (they may still change the shell's)
/// usually implement [`StatelessBuiltin`] instead.
///
/// # Shells
///
/// A builtin is implemented for the shells it runs in, by their extensions
/// type (`SE`). One meant to run in any shell implements it for every `SE`
/// (`impl<SE: ShellExtensions> Builtin<SE> for Tally`), as brush's own builtins
/// do; an embedder's builtin can instead implement it for its own shell's type
/// alone (`impl Builtin<MyExtensions> for Tally`), to rely on that type.
///
/// # State
///
/// State an embedder's builtin needs belongs in the builtin value.
///
/// A registration shares its value with every copy of the shell, including
/// subshells (`( ... )`, a pipeline stage, `$( ... )`). So, unlike the shell's
/// own state, a change a builtin makes to its own state (behind interior
/// mutability) in a subshell is seen by the shell that started it. Keep state
/// that should follow subshell rules in the shell, in a variable say.
///
/// # Cancellation
///
/// The future `execute` returns may be dropped before it completes, at any
/// `.await`: by the shell, or by an embedder cancelling the command that's
/// running it. Leave the builtin's own state consistent at each `.await`.
/// Changes it has already made to the shell (to variables, say) stay made.
/// Dropping the future doesn't stop blocking work it started (a synchronous
/// read, or a task given to `spawn_blocking`), so a builtin that may wait a
/// long time should wait on something that is safe to cancel.
pub trait Builtin<SE: extensions::ShellExtensions>: Send + Sync + 'static {
    /// What one invocation's arguments parse to.
    type Args: Send;

    /// Returns the builtin's help, as invoked under `name`; see
    /// [`ParseArgs::help`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    fn help(&self, name: &str) -> Help;

    /// Returns the builtin's help rendered for a terminal of `style`, or `None`
    /// for the shell to print [`help`](Self::help) as it is; see
    /// [`ParseArgs::terminal_help`]. By default, `None`.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `style` - The terminal's width and color depth.
    fn terminal_help(&self, name: &str, style: &TerminalStyle) -> Option<Help> {
        let _ = (name, style);
        None
    }

    /// Returns how the builtin relates to declarations; see [`Declarations`].
    fn declarations(&self) -> Declarations {
        Declarations::No
    }

    /// Parses an invocation's arguments. Sees only the words, not the shell's
    /// state; see [`Invocation`].
    ///
    /// # Arguments
    ///
    /// * `invocation` - The invocation.
    ///
    /// # Errors
    ///
    /// Returns an error if the arguments are invalid, or if they ask for help;
    /// the shell reports it (see [`UsageError`]).
    fn parse(&self, invocation: Invocation<'_>) -> Result<Self::Args, UsageError>;

    /// Runs one invocation.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the builtin is running.
    /// * `args` - The parsed arguments.
    ///
    /// # Errors
    ///
    /// Returns an error for a failure that stops the builtin; the shell
    /// reports it. See [Failure](crate::builtins#failure).
    fn execute(
        &self,
        context: commands::ExecutionContext<'_, SE>,
        args: Self::Args,
    ) -> impl Future<Output = Result<results::ExecutionResult, error::Error>> + Send;
}

/// A builtin with no state of its own: its arguments type is the whole
/// builtin.
///
/// Each invocation is parsed (by its [`ParseArgs`] implementation) into a
/// value, which is then executed, and consumed. It may still change the
/// shell's state, as `cd` does. Register one with
/// [`Registration::stateless`]; it then runs as a [`Builtin`]. A builtin with
/// state or configuration of its own implements [`Builtin`] instead. Like
/// [`Builtin`], it's implemented for the shells it runs in (see
/// [Shells](Builtin#shells)): usually every one, as
/// `impl<SE: ShellExtensions> StatelessBuiltin<SE> for Cd`.
pub trait StatelessBuiltin<SE: extensions::ShellExtensions>: ParseArgs + 'static {
    /// How the builtin relates to declarations; see [`Declarations`]. A
    /// declaration utility's parser should keep its operands as they arrive,
    /// since a declaration flattened to a word loses an array value's elements.
    const DECLARATIONS: Declarations = Declarations::No;

    /// Executes the built-in command in the provided context, consuming its
    /// parsed arguments. The shell has already reported any problem
    /// [`ParseArgs::parse`] found.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the command is being executed.
    ///
    /// # Errors
    ///
    /// Returns an error for a failure that stops the builtin; the shell
    /// reports it. See [Failure](crate::builtins#failure).
    // NOTE: we use desugared async here because we need a Send marker
    fn execute(
        self,
        context: commands::ExecutionContext<'_, SE>,
    ) -> impl Future<Output = Result<results::ExecutionResult, error::Error>> + Send;
}

/// The [`Builtin`] for a [`StatelessBuiltin`].
struct Typed<C>(PhantomData<fn() -> C>);

impl<SE: extensions::ShellExtensions, C: StatelessBuiltin<SE>> Builtin<SE> for Typed<C> {
    type Args = C;

    fn help(&self, name: &str) -> Help {
        C::help(name)
    }

    fn terminal_help(&self, name: &str, style: &TerminalStyle) -> Option<Help> {
        C::terminal_help(name, style)
    }

    fn declarations(&self) -> Declarations {
        C::DECLARATIONS
    }

    fn parse(&self, invocation: Invocation<'_>) -> Result<C, UsageError> {
        C::parse(invocation)
    }

    async fn execute(
        &self,
        context: commands::ExecutionContext<'_, SE>,
        command: C,
    ) -> Result<results::ExecutionResult, error::Error> {
        command.execute(context).await
    }
}

/// The object-safe form of [`Builtin`] that a [`Registration`] holds.
trait DynBuiltin<SE: extensions::ShellExtensions>: Send + Sync {
    fn help(&self, name: &str) -> Help;

    fn terminal_help(&self, name: &str, style: &TerminalStyle) -> Option<Help>;

    fn declarations(&self) -> Declarations;

    /// Parses and runs one invocation. Reports a problem with the arguments
    /// itself; attributes an error the builtin returns to it.
    fn invoke<'a>(
        &'a self,
        context: commands::ExecutionContext<'a, SE>,
        args: Vec<CommandArg>,
    ) -> BoxFuture<'a, Result<results::ExecutionResult, error::Error>>;
}

impl<SE: extensions::ShellExtensions, B: Builtin<SE>> DynBuiltin<SE> for B {
    fn help(&self, name: &str) -> Help {
        Builtin::help(self, name)
    }

    fn terminal_help(&self, name: &str, style: &TerminalStyle) -> Option<Help> {
        Builtin::terminal_help(self, name, style)
    }

    fn declarations(&self) -> Declarations {
        Builtin::declarations(self)
    }

    fn invoke<'a>(
        &'a self,
        context: commands::ExecutionContext<'a, SE>,
        args: Vec<CommandArg>,
    ) -> BoxFuture<'a, Result<results::ExecutionResult, error::Error>> {
        Box::pin(async move {
            let invocation = Invocation::new(context.command_name(), args);
            let parsed = match self.parse(invocation) {
                Ok(parsed) => parsed,
                Err(error) => return Ok(report_usage_error(self, &context, &error)),
            };

            let name = context.command_name().to_owned();
            self.execute(context, parsed)
                .await
                .map_err(|e| attribute(&name, e))
        })
    }
}

/// A builtin as registered with a shell, with the facts the shell keeps about
/// it. Cloning a registration shares the builtin.
pub struct Registration<SE: extensions::ShellExtensions> {
    builtin: Arc<dyn DynBuiltin<SE>>,

    /// Whether the registration is enabled (see `enable`).
    enabled: bool,
}

impl<SE: extensions::ShellExtensions> Clone for Registration<SE> {
    fn clone(&self) -> Self {
        Self {
            builtin: Arc::clone(&self.builtin),
            enabled: self.enabled,
        }
    }
}

impl<SE: extensions::ShellExtensions> Registration<SE> {
    /// Returns a registration for `builtin`. For a [`StatelessBuiltin`], see
    /// [`Registration::stateless`].
    ///
    /// # Arguments
    ///
    /// * `builtin` - The builtin.
    pub fn new(builtin: impl Builtin<SE>) -> Self {
        Self {
            builtin: Arc::new(builtin),
            enabled: true,
        }
    }

    /// Returns a registration for the [`StatelessBuiltin`] `B`.
    pub fn stateless<B: StatelessBuiltin<SE>>() -> Self {
        Self::new(Typed::<B>(PhantomData))
    }

    /// Returns the builtin's help, as invoked under `name`, as the shell
    /// prints it: rendered for a terminal of `style`, if there is one and the
    /// builtin renders it, and otherwise plain. See
    /// [`ParseArgs::terminal_help`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `style` - The terminal the help is printed to, if it is; see
    ///   [`crate::ExecutionContext::help_style`]. `None` for plain help.
    pub fn help(&self, name: &str, style: Option<&TerminalStyle>) -> Help {
        help_for(self.builtin.as_ref(), name, style)
    }

    /// Returns how the builtin relates to declarations.
    pub fn declarations(&self) -> Declarations {
        self.builtin.declarations()
    }

    /// Returns whether the registration is enabled. A registration starts
    /// enabled; `enable -n` disables one, and a disabled builtin isn't found by
    /// name.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables the registration, as `enable` and `enable -n` do.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether the builtin should be enabled.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// The special builtins' names: POSIX's, and, as in bash, `source`.
const SPECIAL_BUILTINS: [&str; 16] = [
    ":", ".", "break", "continue", "eval", "exec", "exit", "export", "readonly", "return", "set",
    "shift", "source", "times", "trap", "unset",
];

/// Returns whether `name` is a special builtin's name.
///
/// The special builtins are POSIX's (`break`, `:`, `.`, `continue`, `eval`,
/// `exec`, `exit`, `export`, `readonly`, `return`, `set`, `shift`, `times`,
/// `trap`, `unset`) and, as in bash, `source`.
///
/// Being special is a matter of the name, as in bash, not of the builtin: any
/// builtin registered under one of these names is special, and no other is.
/// In POSIX mode, a special builtin is found before a function of the same
/// name, no function can be given its name, and an error it returns ends a
/// non-interactive shell (see [Failure](crate::builtins#failure)).
///
/// # Arguments
///
/// * `name` - The name a builtin is registered or invoked under.
pub fn is_special(name: &str) -> bool {
    SPECIAL_BUILTINS.contains(&name)
}

/// Attributes an error a builtin returned to the builtin, keeping whether it's
/// fatal. An error already attributed to a builtin (one that `builtin` or
/// `command` ran for it, say) keeps that attribution, as in bash.
fn attribute(name: &str, error: error::Error) -> error::Error {
    if matches!(error.kind(), error::ErrorKind::BuiltinError { .. }) {
        return error;
    }
    let fatal = error.is_fatal();
    let error = error::Error::from(error::ErrorKind::BuiltinError {
        builtin: name.to_owned(),
        error,
    });
    if fatal { error.into_fatal() } else { error }
}

/// Reports a problem with a builtin's arguments: prints it on the builtin's
/// standard error, attributed to the builtin, with its usage line if called
/// for, or, for a help request, prints the builtin's help on its standard
/// output. Returns the error's exit code as the command's result.
///
/// Only a failed parse gets here, so a builtin that parses pays for none of
/// this: not the help's terminal style, nor its output files.
fn report_usage_error<SE: extensions::ShellExtensions>(
    builtin: &dyn DynBuiltin<SE>,
    context: &commands::ExecutionContext<'_, SE>,
    error: &UsageError,
) -> results::ExecutionResult {
    let name = context.command_name();
    if error.is_help_request() {
        let help = help_for(builtin, name, context.help_style().as_ref());
        let _ = context
            .stdout()
            .write_all(help.render(name, HelpForm::Full).as_bytes());
    } else {
        let text = render_usage_error(name, error, || builtin.help(name));
        let _ = context.stderr().write_all(text.as_bytes());
    }
    error.exit_code().into()
}

/// Renders a problem with a builtin's arguments as the shell reports it:
/// attributed to the builtin, with its usage line if called for.
fn render_usage_error(name: &str, error: &UsageError, help: impl FnOnce() -> Help) -> String {
    if error.shows_usage_line() {
        format!("{name}: {error}\n{name}: usage: {}\n", help().synopsis)
    } else {
        format!("{name}: {error}\n")
    }
}

/// Returns `builtin`'s help for a terminal of `style`, if there is one and it
/// renders it, and otherwise plain.
fn help_for<SE: extensions::ShellExtensions>(
    builtin: &dyn DynBuiltin<SE>,
    name: &str,
    style: Option<&TerminalStyle>,
) -> Help {
    style
        .and_then(|style| builtin.terminal_help(name, style))
        .unwrap_or_else(|| builtin.help(name))
}

/// Runs a builtin, applying the shell's policy to how it fails:
///
/// * A problem with its arguments (a [`UsageError`]) is printed on the
///   builtin's standard error, attributed to it, with its usage line if called
///   for, and a help request prints the builtin's help on standard output.
///   Either way, the command fails with the error's exit code.
/// * Any other error is attributed to the builtin. A broken pipe isn't
///   reported. If `as_special`, an error is fatal for a special builtin in
///   POSIX mode.
///
/// # Arguments
///
/// * `registration` - The builtin's registration.
/// * `context` - The context to run it in; names the builtin.
/// * `args` - The arguments following the builtin's name.
/// * `as_special` - Whether a special builtin keeps its special properties.
///   Running a builtin through `builtin` drops them.
pub(crate) async fn invoke<SE: extensions::ShellExtensions>(
    registration: &Registration<SE>,
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
    as_special: bool,
) -> Result<results::ExecutionResult, error::Error> {
    // In POSIX mode, special builtins that return errors are to be treated as
    // fatal.
    let posix_fatal =
        as_special && is_special(context.command_name()) && context.shell.options().posix_mode;

    match registration.builtin.invoke(context, args).await {
        Ok(result) => Ok(result),
        Err(error) => {
            // Broken pipe errors should silently return the appropriate exit code.
            if let Some(io_err) = error.as_io_error()
                && io_err.kind() == std::io::ErrorKind::BrokenPipe
            {
                return Ok(results::ExecutionExitCode::from(io_err).into());
            }
            Err(if posix_fatal {
                error.into_fatal()
            } else {
                error
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_builtins_are_bash_s() {
        for name in [".", ":", "export", "readonly", "source", "times", "unset"] {
            assert!(is_special(name), "{name}");
        }
        for name in ["declare", "local", "cd", "command", "builtin", "Export", ""] {
            assert!(!is_special(name), "{name}");
        }
    }

    #[test]
    fn declarations_render_as_the_word_they_came_from() {
        let mut declaration = Declaration::new("a");
        assert_eq!(declaration.to_string(), "a");

        declaration.index = Some("1".into());
        declaration.append = true;
        declaration.value = Some(variables::ShellValueLiteral::Scalar("v".into()));
        assert_eq!(declaration.to_string(), "a[1]+=v");

        let mut declaration = Declaration::new("a");
        declaration.value = Some(variables::ShellValueLiteral::Array(
            variables::ArrayLiteral(vec![(None, "x y".into()), (Some("k".into()), "v".into())]),
        ));
        assert_eq!(declaration.to_string(), "a=(x y [k]=v)");
    }

    #[test]
    fn declarations_read_from_words_as_bash_reads_them() {
        let read = |word| {
            let d = Declaration::from_word(word);
            let value = match d.value {
                Some(variables::ShellValueLiteral::Scalar(value)) => Some(value),
                Some(variables::ShellValueLiteral::Array(_)) => Some("<array>".into()),
                None => None,
            };
            (d.name, d.index, d.append, value)
        };
        let scalar = |s: &str| Some(s.to_owned());
        assert_eq!(read("x=1"), ("x".into(), None, false, scalar("1")));
        assert_eq!(read("x+=1"), ("x".into(), None, true, scalar("1")));
        assert_eq!(read("x="), ("x".into(), None, false, scalar("")));
        assert_eq!(read("x"), ("x".into(), None, false, None));
        assert_eq!(
            read("m[a=b]=1"),
            ("m".into(), Some("a=b".into()), false, scalar("1"))
        );
        assert_eq!(
            read("a[i[0]]+=v"),
            ("a".into(), Some("i[0]".into()), true, scalar("v"))
        );
        // An array value in a word is just text.
        assert_eq!(read("x=(1 2)"), ("x".into(), None, false, scalar("(1 2)")));
        // The name isn't validated.
        assert_eq!(read("a b=1"), ("a b".into(), None, false, scalar("1")));
        // Any other shape is all name, for the builtin to reject.
        for word in ["x+y=1", "m[1", "m[1]x"] {
            assert_eq!(read(word), (word.into(), None, false, None), "{word}");
        }
    }

    #[test]
    fn usage_errors_render_with_their_usage_line() {
        let help = || Help::new("t [-a]", "Does t.");
        assert_eq!(
            render_usage_error("t", &UsageError::new("bad"), help),
            "t: bad\n"
        );
        assert_eq!(
            render_usage_error("t", &UsageError::invalid_option("-z"), help),
            "t: -z: invalid option\nt: usage: t [-a]\n"
        );
    }
}
