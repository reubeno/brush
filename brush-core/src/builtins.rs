//! Facilities for implementing and managing builtins

pub use futures::future::BoxFuture;
use std::ffi::{OsStr, OsString};
use std::io::Write;

use crate::{BuiltinError, CommandArg, commands, error, extensions, results};

/// An owned error produced when a command line fails to parse.
///
/// Parse errors produced by `usage` borrow from the argv they were given, so they
/// must be rendered before those buffers go away; this type carries the rendered
/// text along with enough information for a caller to pick an output stream and
/// an exit code.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Pre-rendered message ready to be written out.
    text: String,
    /// Classification of what went wrong (or was requested).
    kind: ParseErrorKind,
}

/// Classifies the outcome of a parse that produced a [`ParseError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Help was requested (`-h`/`--help`); the text is a help page.
    Help,
    /// Version information was requested; the text identifies the program.
    Version,
    /// The command line could not be parsed (or help was required but missing).
    Failure,
}

impl ParseError {
    const fn new(text: String, kind: ParseErrorKind) -> Self {
        Self { text, kind }
    }

    /// Writes the rendered text to the stream appropriate for its kind:
    /// stdout for help/version requests, stderr otherwise.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error encountered while writing.
    pub fn print(&self) -> std::io::Result<()> {
        match self.kind {
            ParseErrorKind::Help | ParseErrorKind::Version => {
                std::io::stdout().write_all(self.text.as_bytes())
            }
            ParseErrorKind::Failure => std::io::stderr().write_all(self.text.as_bytes()),
        }
    }

    /// Returns the exit code a standalone program should exit with after
    /// printing this error: 0 for help/version requests, 2 otherwise
    /// (matching clap's convention).
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self.kind {
            ParseErrorKind::Help | ParseErrorKind::Version => 0,
            ParseErrorKind::Failure => 2,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for ParseError {}

/// Bridge between types that derive [`usage::Cli`](https://docs.rs/usage-rs) and this
/// module's generic machinery.
///
/// usage's derive generates *inherent* methods (`parse_from`, `spec`, ...) rather than
/// trait implementations, so there is no usage-provided trait to bound on. Types parsed
/// by this module forward to their generated inherent methods via this small trait,
/// typically using the [`impl_usage_parse!`](crate::impl_usage_parse) macro.
pub trait UsageParse: Sized {
    /// Parses the given words (which should *not* include a program name) into `Self`.
    ///
    /// # Arguments
    ///
    /// * `argv` - The words to parse.
    fn parse_argv<'v>(argv: &[&'v OsStr]) -> Result<Self, usage::argv::Error<'static, 'v>>;

    /// Returns the static spec metadata generated for this type.
    #[doc(hidden)]
    fn usage_spec() -> &'static usage::spec::Spec<'static>;
}

/// Implements [`UsageParse`] for a type that derives `usage::Cli`.
#[macro_export]
macro_rules! impl_usage_parse {
    ($ty:ty) => {
        impl $crate::builtins::UsageParse for $ty {
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

/// Parses pre-converted words into `T`, rendering any failure into an owned error.
///
/// The first word is taken to be the command's name and is not parsed, mirroring
/// the `argv[0]` convention of the previous clap-based implementation.
fn parse_words<T: UsageParse>(mut words: Vec<String>) -> Result<T, ParseError> {
    if !words.is_empty() {
        words.remove(0);
    }
    let os_args: Vec<OsString> = words.into_iter().map(Into::into).collect();
    let refs: Vec<&OsStr> = os_args.iter().map(OsString::as_os_str).collect();

    match T::parse_argv(&refs) {
        Ok(parsed) => Ok(parsed),
        Err(err) => Err(render_parse_error(T::usage_spec(), &refs, &err)),
    }
}

/// Renders a failed parse into an owned, printable error, handling help and version
/// requests (which are not failures) along the way.
fn render_parse_error(
    spec: &usage::spec::Spec<'_>,
    argv: &[&OsStr],
    err: &usage::Error<'_, '_>,
) -> ParseError {
    use usage::Error;

    let (text, kind) = match err {
        Error::Help { cmd, long } => (
            usage::help::render_styled(spec, cmd, *long, usage::help::Style::auto())
                .unwrap_or_default(),
            ParseErrorKind::Help,
        ),
        Error::HelpAll { cmd } => (
            usage::help::render_styled(spec, cmd, true, usage::help::Style::auto())
                .unwrap_or_default(),
            ParseErrorKind::Help,
        ),
        // clap prints the *short* help page to stderr and exits non-zero in this case;
        // preserve that contract.
        Error::MissingArgsHelp { cmd } => (
            usage::help::render_styled(spec, cmd, false, usage::help::Style::auto_stderr())
                .unwrap_or_default(),
            ParseErrorKind::Failure,
        ),
        Error::Version { .. } => {
            let mut text = String::new();
            if let Some(bin) = spec.bin.filter(|b| !b.is_empty()) {
                text.push_str(bin);
                text.push(' ');
            }
            if let Some(version) = spec.version.or(spec.long_version) {
                text.push_str(version);
            }
            text.push('\n');
            (text, ParseErrorKind::Version)
        }
        _ => (
            usage::render_failure(spec, argv, err),
            ParseErrorKind::Failure,
        ),
    };

    ParseError::new(text, kind)
}

/// Type of a function implementing a built-in command.
///
/// # Arguments
///
/// * The context in which the command is being executed.
/// * The arguments to the command.
#[allow(type_alias_bounds)]
pub type CommandExecuteFunc<SE: extensions::ShellExtensions> =
    fn(
        commands::ExecutionContext<'_, SE>,
        Vec<commands::CommandArg>,
    ) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>>;

/// Type of a function to retrieve help content for a built-in command.
///
/// # Arguments
///
/// * `name` - The name of the command.
/// * `content_type` - The type of content to retrieve.
/// * `options` - Additional options for content retrieval.
pub type CommandContentFunc =
    fn(&str, ContentType, &ContentOptions) -> Result<String, error::Error>;

/// Trait implemented by built-in shell commands.
pub trait Command: UsageParse {
    /// The error type returned by the command.
    type Error: BuiltinError + 'static;

    /// Instantiates the built-in command with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments to the command.
    fn new<I>(args: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = String>,
        Self: Sized,
    {
        let args: Vec<String> = args.into_iter().collect();

        if !Self::takes_plus_options() {
            parse_words::<Self>(args)
        } else {
            // N.B. usage doesn't support named options like '+x'. To work around this, we
            // establish a pattern of renaming them.
            let updated_args = args
                .into_iter()
                .map(|arg| match arg.strip_prefix('+') {
                    Some(plus_options) => plus_options.chars().map(|c| format!("--+{c}")).collect(),
                    None => vec![arg],
                })
                .collect::<Vec<Vec<String>>>()
                .concat();

            parse_words::<Self>(updated_args)
        }
    }

    /// Returns whether or not the command takes options with a leading '+' or '-' character.
    fn takes_plus_options() -> bool {
        false
    }

    /// Executes the built-in command in the provided context.
    ///
    /// # Arguments
    ///
    /// * `context` - The context in which the command is being executed.
    // NOTE: we use desugared async here because we need a Send marker
    fn execute<SE: extensions::ShellExtensions>(
        &self,
        context: commands::ExecutionContext<'_, SE>,
    ) -> impl std::future::Future<Output = Result<results::ExecutionResult, Self::Error>>
    + std::marker::Send;

    /// Returns the textual help content associated with the command.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    /// * `content_type` - The type of content to retrieve.
    /// * `options` - Additional options for content retrieval.
    fn get_content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error> {
        let spec = Self::usage_spec();
        let cmd = spec.root.cmd;

        let style = if options.colorized {
            usage::help::Style::COLOURED
        } else {
            usage::help::Style::PLAIN
        };

        let s = match content_type {
            ContentType::DetailedHelp => usage::help::render_styled(spec, cmd, true, style)
                .unwrap_or_else(|| "no help available".to_string()),
            ContentType::ShortUsage => get_builtin_short_usage::<Self>(name),
            ContentType::ShortDescription => get_builtin_short_description(name, spec),
            ContentType::ManPage => get_builtin_man_page(name)?,
        };

        Ok(s)
    }
}

/// Trait implemented by built-in shell commands that take specially handled declarations
/// as arguments.
pub trait DeclarationCommand: Command {
    /// Stores the declarations within the command instance.
    ///
    /// # Arguments
    ///
    /// * `declarations` - The declarations to store.
    fn set_declarations(&mut self, declarations: Vec<commands::CommandArg>);
}

/// Type of help content, typically associated with a built-in command.
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

/// Encapsulates a registration for a built-in command.
#[derive(Clone)]
pub struct Registration<SE: extensions::ShellExtensions> {
    /// Function to execute the builtin.
    pub execute_func: CommandExecuteFunc<SE>,

    /// Function to retrieve the builtin's content/help text.
    pub content_func: CommandContentFunc,

    /// Has this registration been disabled?
    pub disabled: bool,

    /// Is the builtin classified as "special" by specification?
    pub special_builtin: bool,

    /// Is this builtin one that takes specially handled declarations?
    pub declaration_builtin: bool,
}

impl<SE: extensions::ShellExtensions> Registration<SE> {
    /// Updates the given registration to mark it for a special builtin.
    #[must_use]
    pub const fn special(self) -> Self {
        Self {
            special_builtin: true,
            ..self
        }
    }
}

fn get_builtin_man_page(_name: &str) -> Result<String, error::Error> {
    error::unimp("man page rendering is not yet implemented")
}

fn get_builtin_short_description(name: &str, spec: &usage::spec::Spec<'_>) -> String {
    let about = spec
        .about
        .or(spec.root.about)
        .map_or_else(String::new, std::string::ToString::to_string);

    std::format!("{name} - {about}\n")
}

fn get_builtin_short_usage<T: Command>(name: &str) -> String {
    let spec = T::usage_spec();
    let cmd = spec.root.cmd;
    let mut usage = String::new();

    let mut needs_space = false;

    let mut optional_short_opts = vec![];
    let mut required_short_opts = vec![];
    for (flag, meta) in cmd.flags.iter().zip(spec.root.flags) {
        if meta.hide {
            continue;
        }

        for &c in flag.shorts {
            let c = char::from(c);
            if flag.takes_value || meta.required {
                required_short_opts.push(c);
            } else {
                optional_short_opts.push(c);
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

    for (pos, meta) in cmd.args.iter().zip(spec.root.args) {
        if meta.hide {
            continue;
        }

        if !pos.required {
            if needs_space {
                usage.push(' ');
            }

            usage.push('[');
            needs_space = false;
        }

        for name in pos.name.split(' ') {
            if needs_space {
                usage.push(' ');
            }

            usage.push_str(name);
            needs_space = true;
        }

        if !pos.required {
            usage.push(']');
            needs_space = true;
        }
    }

    std::format!("{name}: {name} {usage}\n")
}

/// Parses the given words into `T`, treating the first word as the command name.
///
/// # Errors
///
/// Returns a [`ParseError`] if the words fail to parse.
pub fn parse_command_args<T: UsageParse>(
    words: impl IntoIterator<Item = String>,
) -> Result<T, ParseError> {
    parse_words(words.into_iter().collect())
}

/// Splits the given arguments at the first standalone `--` and parses the words
/// before it into `T`.
///
/// This function exists to preserve bash-like treatment of `--` by the shell's own
/// command-line parsing, where everything from the first `--` onward is passed through
/// verbatim to the script or builtin.
///
/// This function is used to parse arguments in builtins such as
/// `crate::echo::EchoCommand`
pub fn try_parse_known<T: UsageParse>(
    args: impl IntoIterator<Item = String>,
) -> Result<(T, Option<impl Iterator<Item = String>>), ParseError> {
    let mut args = args.into_iter();
    let mut hyphen = None;
    let args_before_hyphen = args.by_ref().take_while(|a| {
        let is_hyphen = a == "--";
        if is_hyphen {
            hyphen = Some(a.clone());
        }
        !is_hyphen
    });
    let collected: Vec<String> = args_before_hyphen.collect();
    let parsed_args = parse_words::<T>(collected)?;

    let raw_args = hyphen.map(|hyphen| std::iter::once(hyphen).chain(args));
    Ok((parsed_args, raw_args))
}

/// A simple command that can be registered as a built-in.
pub trait SimpleCommand {
    /// Returns the content of the built-in command.
    fn get_content(
        name: &str,
        content_type: ContentType,
        options: &ContentOptions,
    ) -> Result<String, error::Error>;

    /// Executes the built-in command.
    fn execute<SE: extensions::ShellExtensions, I: Iterator<Item = S>, S: AsRef<str>>(
        context: commands::ExecutionContext<'_, SE>,
        args: I,
    ) -> Result<results::ExecutionResult, error::Error>;
}

/// Returns a built-in command registration, given an implementation of the
/// `SimpleCommand` trait.
pub fn simple_builtin<B: SimpleCommand + Send + Sync, SE: extensions::ShellExtensions>()
-> Registration<SE> {
    Registration {
        execute_func: exec_simple_builtin::<B, SE>,
        content_func: B::get_content,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
    }
}

/// Returns a built-in command registration, given an implementation of the
/// `Command` trait.
pub fn builtin<B: Command + Send + Sync, SE: extensions::ShellExtensions>() -> Registration<SE> {
    Registration {
        execute_func: exec_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
    }
}

/// Returns a built-in command registration, given an implementation of the
/// `DeclarationCommand` trait. Used for select commands that can take parsed
/// declarations as arguments.
pub fn decl_builtin<B: DeclarationCommand + Send + Sync, SE: extensions::ShellExtensions>()
-> Registration<SE> {
    Registration {
        execute_func: exec_declaration_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: true,
    }
}

#[allow(clippy::too_long_first_doc_paragraph)]
/// Returns a built-in command registration, given an implementation of the
/// `DeclarationCommand` trait that can be default-constructed. The command
/// implementation is expected to implement clap's `Parser` trait solely
/// for help/usage information. Arguments are passed directly to the command
/// via `set_declarations`. This is primarily only expected to be used with
/// select builtin commands that wrap other builtins (e.g., "builtin").
pub fn raw_arg_builtin<
    B: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>() -> Registration<SE> {
    Registration {
        execute_func: exec_raw_arg_builtin::<B, SE>,
        content_func: get_builtin_content::<B>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: true,
    }
}

fn get_builtin_content<T: Command + Send + Sync>(
    name: &str,
    content_type: ContentType,
    options: &ContentOptions,
) -> Result<String, error::Error> {
    T::get_content(name, content_type, options)
}

fn exec_simple_builtin<T: SimpleCommand + Send + Sync, SE: extensions::ShellExtensions>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_simple_builtin_impl::<T, SE>(context, args).await })
}

#[expect(clippy::unused_async)]
async fn exec_simple_builtin_impl<
    T: SimpleCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let plain_args = args.into_iter().map(|arg| match arg {
        CommandArg::String(s) => s,
        CommandArg::Assignment(a) => a.to_string(),
    });

    T::execute(context, plain_args)
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
    let plain_args = args.into_iter().map(|arg| match arg {
        CommandArg::String(s) => s,
        CommandArg::Assignment(a) => a.to_string(),
    });

    let result = T::new(plain_args);
    let command = match result {
        Ok(command) => command,
        Err(e) => {
            let _ = writeln!(context.stderr(), "{e}");
            return Ok(results::ExecutionExitCode::InvalidUsage.into());
        }
    };

    call_builtin(command, context).await
}

fn exec_declaration_builtin<
    T: DeclarationCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_declaration_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_declaration_builtin_impl<
    T: DeclarationCommand + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let mut options = vec![];
    let mut declarations = vec![];

    for (i, arg) in args.into_iter().enumerate() {
        match arg {
            CommandArg::String(s)
                if i == 0 || (s.len() > 1 && (s.starts_with('-') || s.starts_with('+'))) =>
            {
                options.push(s);
            }
            _ => declarations.push(arg),
        }
    }

    let result = T::new(options);
    let mut command = match result {
        Ok(command) => command,
        Err(e) => {
            let _ = writeln!(context.stderr(), "{e}");
            return Ok(results::ExecutionExitCode::InvalidUsage.into());
        }
    };

    command.set_declarations(declarations);

    call_builtin(command, context).await
}

fn exec_raw_arg_builtin<
    T: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<results::ExecutionResult, error::Error>> {
    Box::pin(async move { exec_raw_arg_builtin_impl::<T, SE>(context, args).await })
}

async fn exec_raw_arg_builtin_impl<
    T: DeclarationCommand + Default + Send + Sync,
    SE: extensions::ShellExtensions,
>(
    context: commands::ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<results::ExecutionResult, error::Error> {
    let mut command = T::default();
    command.set_declarations(args);

    call_builtin(command, context).await
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
