//! Facilities for implementing and managing builtins

pub use futures::future::BoxFuture;
use std::io::Write;

pub mod args;

use crate::{BuiltinError, CommandArg, commands, error, extensions, results};
pub use args::{ArgsError, FromArgs};

/// Type of a function implementing a built-in command.
///
/// # Arguments
///
/// * The context in which the command is being executed. The name the built-in
///   was invoked under is `context.command_name`.
/// * The arguments following that name. The name is *not* repeated here.
#[allow(type_alias_bounds)]
pub type CommandExecuteFunc<SE: extensions::ShellExtensions> =
    fn(
        commands::ExecutionContext<'_, SE>,
        Vec<commands::CommandArg>,
    ) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>>;

/// Trait implemented by built-in shell commands.
pub trait Command: FromArgs + HelpContent {
    /// The error type returned by the command. See [`Command::execute`] for when
    /// to return an error rather than a failing [`results::ExecutionResult`].
    type Error: BuiltinError + 'static;

    /// Executes the built-in command in the provided context.
    ///
    /// # Reporting failure
    ///
    /// Argument errors from [`FromArgs`] are reported by the shell before this
    /// is called. For failures found while executing, a builtin can either:
    ///
    /// * **Return a failing exit status.** Write any diagnostic to
    ///   `context.stderr()` in the builtin's own words, then return `Ok` with a
    ///   non-zero status. The shell uses the status as-is.
    /// * **Return an error.** The shell prints it through its error formatter,
    ///   attributed to the builtin by name, and takes the exit status from the
    ///   error's exit-code conversion. For a special builtin in POSIX mode, an
    ///   error is fatal in a non-interactive shell. An I/O error for a broken
    ///   pipe is the exception: it is never printed or fatal, and just sets the
    ///   status.
    ///
    /// A successful result can also redirect control flow through its
    /// `next_control_flow` field, as `break`, `return`, and `exit` do.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the command is being executed.
    ///
    /// # Errors
    ///
    /// Returns an error for a failure the shell should report on the builtin's
    /// behalf; see "Reporting failure" above.
    // NOTE: we use desugared async here because we need a Send marker
    fn execute<SE: extensions::ShellExtensions>(
        &self,
        context: commands::ExecutionContext<'_, SE>,
    ) -> impl std::future::Future<Output = Result<results::ExecutionResult, Self::Error>>
    + std::marker::Send;
}

/// Trait yielding a built-in command's help text.
///
/// The short forms are structured: `help -s` and `help -d` frame them
/// uniformly (as `name: synopsis` and `name - description`), so implementations
/// return only the part that differs. Detailed help is free-form and printed
/// verbatim.
pub trait HelpContent {
    /// Returns the command's synopsis, e.g. `cd [-L|-P] [dir]`. Includes the
    /// command's own name, but no `name: ` prefix and no trailing newline.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the built-in was invoked under.
    fn synopsis(name: &str) -> String;

    /// Returns a one-line description of the command, with no `name - ` prefix
    /// and no trailing newline.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the built-in was invoked under.
    fn description(name: &str) -> String;

    /// Returns the command's full help, rendered however the implementation
    /// likes. Defaults to the synopsis followed by the indented description.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the built-in was invoked under.
    /// * `options` - Options controlling how the content is rendered.
    ///
    /// # Errors
    ///
    /// Returns an error if the help cannot be rendered.
    fn detailed_help(name: &str, _options: &ContentOptions) -> Result<String, error::Error> {
        Ok(format!(
            "{name}: {}\n    {}\n",
            Self::synopsis(name),
            Self::description(name)
        ))
    }
}

/// Options for retrieving built-in command content.
#[derive(Default)]
#[non_exhaustive]
pub struct ContentOptions {
    /// Whether or not the content should be colorized.
    pub colorized: bool,
}

/// Encapsulates a registration for a built-in command.
///
/// Construct one with [`builtin`] for a [`Command`] implementation, or with
/// [`Registration::new`] for a bare function. The execution function and whether
/// the builtin takes declarations are fixed at construction, because the shell
/// relies on the two agreeing.
#[derive(Clone)]
pub struct Registration<SE: extensions::ShellExtensions> {
    /// Function to execute the builtin.
    execute_func: CommandExecuteFunc<SE>,

    /// Functions yielding the builtin's help text; see [`HelpContent`].
    synopsis_func: fn(&str) -> String,
    description_func: fn(&str) -> String,
    detailed_help_func: fn(&str, &ContentOptions) -> Result<String, error::Error>,

    /// Has this registration been disabled?
    disabled: bool,

    /// Is the builtin classified as "special" by specification?
    special: bool,

    /// Whether the builtin takes specially handled declarations. Read during
    /// word expansion, before the builtin runs, to decide whether `name=value`
    /// words become [`CommandArg::Assignment`]; `execute_func` must expect that.
    takes_declarations: bool,
}

impl<SE: extensions::ShellExtensions> Registration<SE> {
    /// Returns a registration for a builtin implemented directly as a function,
    /// rather than through [`Command`]. The builtin receives its arguments as
    /// plain strings; it never takes declarations.
    ///
    /// # Arguments
    ///
    /// * `execute_func` - Function to execute the builtin.
    pub const fn new<H: HelpContent>(execute_func: CommandExecuteFunc<SE>) -> Self {
        Self {
            execute_func,
            synopsis_func: H::synopsis,
            description_func: H::description,
            detailed_help_func: H::detailed_help,
            disabled: false,
            special: false,
            takes_declarations: false,
        }
    }

    /// Returns the function that executes the builtin.
    pub const fn execute_func(&self) -> CommandExecuteFunc<SE> {
        self.execute_func
    }

    /// Returns the builtin's synopsis; see [`HelpContent::synopsis`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    pub fn synopsis(&self, name: &str) -> String {
        (self.synopsis_func)(name)
    }

    /// Returns the builtin's one-line description; see
    /// [`HelpContent::description`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    pub fn description(&self, name: &str) -> String {
        (self.description_func)(name)
    }

    /// Returns the builtin's full help; see [`HelpContent::detailed_help`].
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `options` - Options controlling how the content is rendered.
    ///
    /// # Errors
    ///
    /// Returns an error if the help cannot be rendered.
    pub fn detailed_help(
        &self,
        name: &str,
        options: &ContentOptions,
    ) -> Result<String, error::Error> {
        (self.detailed_help_func)(name, options)
    }

    /// Returns whether the builtin takes specially handled declarations.
    pub const fn takes_declarations(&self) -> bool {
        self.takes_declarations
    }

    /// Returns whether the registration has been disabled (see `enable`).
    pub const fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Enables or disables the registration.
    ///
    /// # Arguments
    ///
    /// * `disabled` - Whether the builtin should be disabled.
    pub const fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    /// Returns whether the builtin is classified as "special" by specification.
    pub const fn is_special(&self) -> bool {
        self.special
    }

    /// Updates the given registration to mark it for a special builtin.
    #[must_use]
    pub const fn special(self) -> Self {
        Self {
            special: true,
            ..self
        }
    }
}

/// Returns a built-in command registration, given an implementation of the
/// `Command` trait.
pub const fn builtin<B: Command + Send + Sync, SE: extensions::ShellExtensions>() -> Registration<SE>
{
    Registration {
        takes_declarations: B::TAKES_DECLARATIONS,
        ..Registration::new::<B>(exec_builtin::<B, SE>)
    }
}

fn exec_builtin<T: Command + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_builtin_impl<T: Command + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let command = match T::from_args(context.command_name.as_str(), args) {
        Ok(command) => command,
        Err(e) => return Ok(report_args_error(&context, &e)),
    };

    call_builtin(command, context).await
}

/// Reports an argument-parsing outcome that stopped the built-in from running,
/// returning the result to yield: the message on stderr, and the usage status.
fn report_args_error<SE: extensions::ShellExtensions>(
    context: &commands::ExecutionContext<'_, SE>,
    error: &ArgsError,
) -> results::ExecutionResult {
    let _ = writeln!(context.stderr(), "{error}");
    results::ExecutionExitCode::InvalidUsage.into()
}

async fn call_builtin(
    command: impl Command,
    context: commands::ExecutionContext<'_, impl extensions::ShellExtensions>,
) -> Result<results::ExecutionResult, error::Error> {
    let builtin_name = context.command_name.clone();
    let result = command
        .execute(context)
        .await
        .map_err(|e| error::ErrorKind::BuiltinError(Box::new(e), builtin_name))?;

    Ok(result)
}
