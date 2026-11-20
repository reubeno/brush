//! The contract between the shell and its builtins.
//!
//! A builtin is a type that is its own parsed arguments: it implements
//! [`ParseArgs`] (an engine adapter can, from a derive) and [`Builtin`],
//! whose `execute` runs it, and is registered with [`Registration::of`].
//! One type can serve several names (`declare` serves four), and tell them
//! apart by the name it was invoked under.
//!
//! `brush-core` names no argument-parsing engine: a builtin that uses clap
//! implements [`ParseArgs`] through the `brush-clap-adapter` crate, and one
//! that parses by hand implements it directly.

pub use futures::future::BoxFuture;
use std::io::Write;

use crate::{BuiltinError, CommandArg, commands, error, extensions, results};

mod args;
mod registry;
mod state;

pub use args::{Invocation, ParseArgs, UsageError};
pub use registry::Registry;
pub(crate) use state::BuiltinStates;

/// Type of help content, typically associated with a built-in command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentType {
    /// Detailed help content for the command.
    DetailedHelp,
    /// Short usage information for the command.
    ShortUsage,
    /// Short description for the command.
    ShortDescription,
    /// man-style help page.
    ManPage,
}

/// Options for retrieving built-in command content.
#[derive(Default)]
pub struct ContentOptions {
    /// Whether or not the content should be colorized.
    pub colorized: bool,
}

/// A builtin's role in declarations: whether its assignment-shaped operands
/// are declarations.
///
/// The shell reads this *before* expanding a command's arguments, because it
/// changes how assignment-shaped words expand: for a declaration utility, a
/// word like `x=$v` expands as an assignment (no field splitting or globbing)
/// and arrives as [`CommandArg::Assignment`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationRole {
    /// None: assignment-shaped words are ordinary words.
    #[default]
    None,
    /// A declaration utility (`declare`, `export`, `local`, `readonly`,
    /// `typeset`).
    Utility,
}

/// The common shape of a builtin: a type that is its own parsed arguments.
///
/// Its parsing half is [`ParseArgs`], which an engine adapter can implement
/// from a derive; [`execute`](Self::execute) is the rest. Register it with
/// [`Registration::of`]. Each invocation is parsed into a value of the type,
/// which is then executed, and consumed. One type may be registered under
/// several names, and tell them apart by the name it was invoked under
/// ([`Invocation::name`], [`ExecutionContext::command_name`](crate::ExecutionContext::command_name)).
///
/// # Shells
///
/// A builtin is implemented for the shells it runs in, by their extensions
/// type (`SE`). One meant to run in any shell implements it for every `SE`
/// (`impl<SE: ShellExtensions> Builtin<SE> for Cd`), as brush's own builtins
/// do; an embedder's builtin can instead implement it for its own shell's
/// type alone (`impl Builtin<MyExtensions> for Cd`), to rely on that type.
///
/// # State
///
/// State a builtin keeps between invocations lives in the shell; see
/// [`ExecutionContext::state_mut`](crate::ExecutionContext::state_mut).
pub trait Builtin<SE: extensions::ShellExtensions>: ParseArgs + 'static {
    /// The error type returned by the builtin.
    type Error: BuiltinError + 'static;

    /// Executes the builtin in the provided context, consuming its parsed
    /// arguments.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the builtin is being executed.
    ///
    /// # Errors
    ///
    /// Returns an error for a failure that stops the builtin; the shell
    /// reports it, attributed to the builtin.
    // NOTE: we use desugared async here because we need a Send marker
    fn execute(
        self,
        context: commands::ExecutionContext<'_, SE>,
    ) -> impl Future<Output = Result<results::ExecutionResult, Self::Error>> + Send;
}

/// How a [`Registration`] runs its builtin: parses and runs one invocation,
/// whose arguments start with the name it was invoked under.
type ExecuteFn<SE> = for<'a> fn(
    commands::ExecutionContext<'a, SE>,
    Vec<CommandArg>,
) -> BoxFuture<'a, Result<results::ExecutionResult, error::Error>>;

/// How a [`Registration`] gets its builtin's help; see [`ParseArgs::content`].
type ContentFn = fn(&str, ContentType, &ContentOptions) -> Result<String, error::Error>;

/// The [`ExecuteFn`] for the builtin `B`.
fn execute_boxed<SE: extensions::ShellExtensions, B: Builtin<SE>>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(invoke::<SE, B>(context, args))
}

/// Parses and runs one invocation of the builtin `B`. `args` starts with the
/// name it was invoked under.
async fn invoke<SE: extensions::ShellExtensions, B: Builtin<SE>>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let mut args = args.into_iter();
    let name = args
        .next()
        .map_or_else(|| context.command_name.clone(), String::from);

    let builtin = match B::parse(Invocation::new(&name, args)) {
        Ok(builtin) => builtin,
        Err(usage) => {
            let _ = if usage.is_help() {
                writeln!(context.stdout(), "{usage}")
            } else {
                writeln!(context.stderr(), "{usage}")
            };
            return Ok(results::ExecutionExitCode::InvalidUsage.into());
        }
    };

    builtin
        .execute(context)
        .await
        .map_err(|e| error::ErrorKind::BuiltinError(Box::new(e), name).into())
}

/// Encapsulates a registration for a built-in command.
#[derive(Clone)]
pub struct Registration<SE: extensions::ShellExtensions> {
    /// Runs the builtin.
    execute: ExecuteFn<SE>,

    /// Gets the builtin's help.
    content: ContentFn,

    /// Whether the builtin is enabled; see [`Registration::is_enabled`].
    enabled: bool,

    /// Is the builtin classified as "special" by specification?
    special_builtin: bool,

    /// The builtin's role in declarations; kept here, as the shell asks for it
    /// while expanding a command's arguments.
    declaration_role: DeclarationRole,
}

impl<SE: extensions::ShellExtensions> Registration<SE> {
    /// Returns an enabled registration for the [`Builtin`] `B`.
    #[doc(alias = "new")]
    pub const fn of<B: Builtin<SE>>() -> Self {
        Self {
            execute: execute_boxed::<SE, B>,
            content: B::content,
            enabled: true,
            special_builtin: false,
            declaration_role: B::DECLARATION_ROLE,
        }
    }

    /// Updates the given registration to mark it for a special builtin.
    #[must_use]
    pub const fn special(self) -> Self {
        Self {
            special_builtin: true,
            ..self
        }
    }

    /// Runs the builtin.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the builtin runs.
    /// * `args` - The arguments to the builtin, starting with its name.
    pub fn execute<'a>(
        &self,
        context: commands::ExecutionContext<'a, SE>,
        args: Vec<commands::CommandArg>,
    ) -> BoxFuture<'a, Result<results::ExecutionResult, error::Error>> {
        (self.execute)(context, args)
    }

    /// Returns the builtin's help content of the given type.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin is registered under.
    /// * `content_type` - The type of content to retrieve.
    /// * `options` - Additional options for content retrieval.
    pub fn content(
        &self,
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error> {
        (self.content)(name, content_type, options)
    }

    /// Returns the builtin's role in declarations.
    pub const fn declaration_role(&self) -> DeclarationRole {
        self.declaration_role
    }

    /// Returns whether the builtin is enabled; `enable -n` disables one.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables the builtin.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether the builtin is to be enabled.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns whether the builtin is classified as "special" by POSIX.
    pub const fn is_special(&self) -> bool {
        self.special_builtin
    }
}
