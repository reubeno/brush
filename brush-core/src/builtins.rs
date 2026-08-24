//! Facilities for implementing and managing builtins

pub use bpaf::Parser;
use bpaf::{Args, ParseFailure};
pub use futures::future::BoxFuture;
use std::ffi::OsStr;
use std::io::Write;

use crate::{BuiltinError, CommandArg, commands, error, extensions, results};

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

/// An error that occurred while parsing a built-in command's arguments.
#[derive(Debug, Clone)]
pub struct BuiltinArgParseError {
    /// The rendered message associated with the parse failure.
    pub message: String,

    /// Whether or not this "error" is actually a request to display help
    /// (or version) information, in which case the message should be
    /// displayed and the builtin should exit successfully.
    pub help_request: bool,
}

impl std::fmt::Display for BuiltinArgParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BuiltinArgParseError {}

fn render_parse_failure(failure: ParseFailure) -> BuiltinArgParseError {
    match failure {
        // Help/version requests are rendered to stdout with a success exit code.
        ParseFailure::Stdout(doc, full) => BuiltinArgParseError {
            message: doc.monochrome(full),
            help_request: true,
        },
        ParseFailure::Completion(s) => BuiltinArgParseError {
            message: s,
            help_request: true,
        },
        // Everything else is a usage error.
        ParseFailure::Stderr(doc) => BuiltinArgParseError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Splits an argument list into the leading section of options (to be parsed)
/// and the trailing section of operands (captured verbatim), following
/// shell-style option termination rules:
///
/// * Parsing stops at the first `--`, which acts purely as an option
///   terminator (i.e., it is dropped from the output).
/// * Parsing stops at the first operand (a token that does not look like an
///   option), which starts the trailing section.
/// * Options listed in `value_takers` consume a following value, either
///   attached to the same token or as the next token (even if it looks like
///   an option). Short options are given as the characters (e.g., `"dnOsu"`),
///   while long options are given by full name (e.g., `"--config"`).
///
/// # Arguments
///
/// * `args` - The arguments to split.
/// * `value_takers` - The options that take a value.
#[must_use]
pub fn split_option_section(
    args: &[String],
    value_shorts: &str,
    value_longs: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();

        // N.B. A bare `--` acts purely as an option terminator: it is removed
        // from the option section and placed at the front of the trailing
        // section, where commands may interpret it as they see fit.
        if arg == "--" {
            return (args[..i].to_vec(), args[i..].to_vec());
        }

        if is_long_option(arg) {
            if value_longs.contains(&arg) {
                // A long option known to take a value in a separate token.
                i += 2;
            } else {
                // Any other long-style option (possibly with an attached
                // value).
                i += 1;
            }
        } else if is_short_or_plus_option(arg, '-', value_shorts) {
            // A group of short options, possibly with an attached value.
            i += short_group_token_count(arg.strip_prefix('-').unwrap_or(""), value_shorts);
        } else if is_short_or_plus_option(arg, '+', value_shorts) {
            // A group of plus-style options (e.g., `set +vx`), possibly with
            // an attached value.
            i += short_group_token_count(arg.strip_prefix('+').unwrap_or(""), value_shorts);
        } else {
            // An operand; everything from here on is captured verbatim.
            return (args[..i].to_vec(), args[i..].to_vec());
        }
    }

    (args.to_vec(), Vec::new())
}

/// Returns whether the given token looks like a long option, i.e., `--` followed
/// by a name of word characters, optionally with an attached `=value`.
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

/// Returns whether the given token looks like a group of short (or
/// plus-style) options, i.e., a leading `-` or `+` followed by one or more
/// alphabetic characters, optionally ending in an attached value beginning at
/// the first value-taking option character. Tokens like `----------------`,
/// `-9223372036854775808`, or `-e hello world` do not qualify and are treated
/// as operands.
fn is_short_or_plus_option(arg: &str, lead: char, value_shorts: &str) -> bool {
    let Some(group) = arg.strip_prefix(lead) else {
        return false;
    };

    let mut saw_value_taker = false;
    for c in group.chars() {
        if c.is_whitespace() {
            return false;
        }

        if saw_value_taker {
            // The remainder of the token is an attached value.
            continue;
        }

        if value_shorts.contains(c) {
            saw_value_taker = true;
        } else if !c.is_alphabetic() {
            return false;
        }
    }

    !group.is_empty()
}

/// Returns the number of tokens occupied by a short (or plus-style) option
/// group: one if any value-taking option in the group has its value attached
/// or takes no value, and two if the last option in the group takes a value
/// in a separate token.
fn short_group_token_count(group: &str, value_shorts: &str) -> usize {
    let char_count = group.chars().count();
    for (j, c) in group.chars().enumerate() {
        if value_shorts.contains(c) {
            return if j == char_count - 1 { 2 } else { 1 };
        }
    }

    1
}

/// Trait implemented by built-in shell commands.
pub trait Command: Sized {
    /// The error type returned by the command.
    type Error: BuiltinError + 'static;

    /// Returns the parser used to interpret the command's arguments.
    ///
    /// Implementations are expected to use `bpaf`'s combinatoric or derive
    /// APIs. The returned parser is wrapped in an [`bpaf::OptionParser`] by
    /// the default implementations of [`Command::new`] and
    /// [`Command::get_content`], which also add standard `--help` handling.
    fn parser() -> impl Parser<Self> + 'static;

    /// Returns a short, one-line description of the command, used by the
    /// `help` builtin.
    fn about() -> &'static str {
        ""
    }

    /// Returns a short synopsis of the command's arguments (excluding the
    /// command name), used by the `help` builtin; e.g., `"[-abc] [NAME]..."`.
    fn synopsis() -> &'static str {
        ""
    }

    /// Returns whether or not the command takes options with a leading '+' character.
    fn takes_plus_options() -> bool {
        false
    }

    /// Returns whether or not the command captures all remaining arguments,
    /// verbatim, after its options.
    fn takes_trailing_args() -> bool {
        false
    }

    /// Returns the characters of short options that take a value; used when
    /// deciding where the option section ends for commands that take trailing
    /// arguments; e.g., `"dnOsu"`.
    fn value_taking_short_options() -> &'static str {
        ""
    }

    /// Instantiates the built-in command with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments to the command.
    fn new<I>(args: I) -> Result<Self, BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself and is not part
        // of the arguments to parse.
        if !args.is_empty() {
            args.remove(0);
        }

        // Expand groups of plus-style options (e.g., `set +vx`) into
        // individually recognizable tokens (e.g., `+v +x`), mirroring how the
        // shell tokenizes option groups.
        let args = if Self::takes_plus_options() {
            expand_plus_option_groups(args)
        } else {
            args
        };

        if Self::takes_trailing_args() {
            let (options, trailing) =
                split_option_section(&args, Self::value_taking_short_options(), &[]);

            let mut command = run_parser::<Self>(&options)?;
            command.set_trailing_args(trailing);

            Ok(command)
        } else {
            run_parser::<Self>(&args)
        }
    }

    /// Stores trailing (verbatim) arguments captured by [`Command::new`] for
    /// commands where [`Command::takes_trailing_args`] returns `true`.
    ///
    /// # Arguments
    ///
    /// * `args` - The trailing arguments.
    fn set_trailing_args(&mut self, _args: Vec<String>) {}

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
        let _ = options;

        let s = match content_type {
            ContentType::DetailedHelp => detailed_help::<Self>(name)?,
            ContentType::ShortUsage => format!("{name}: {name} {}\n", Self::synopsis()),
            ContentType::ShortDescription => format!("{name} - {}\n", Self::about()),
            ContentType::ManPage => get_builtin_man_page(name)?,
        };

        Ok(s)
    }
}

/// Renders the given command's detailed help text.
fn detailed_help<T: Command>(name: &str) -> Result<String, error::Error> {
    // N.B. We trigger bpaf's --help handling to render the help content since
    // rendered help text is not otherwise exposed via the public API.
    let help_args = [OsStr::new("--help")];
    let help_request = Args::from(&help_args[..]).set_name(name);
    match T::parser().to_options().run_inner(help_request) {
        Err(failure) => Ok(render_parse_failure(failure).message),
        Ok(_) => Err(error::ErrorKind::Unimplemented("unexpectedly parsed help request").into()),
    }
}

/// Expands groups of plus-style options (e.g., `+vx`) into individually
/// recognizable tokens (e.g., `+v` and `+x`), mirroring how the shell
/// tokenizes option groups. Tokens that do not start with a single `+` are
/// passed through unchanged.
fn expand_plus_option_groups(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .flat_map(|arg| {
            if let Some(plus_options) = arg.strip_prefix('+').filter(|g| !g.is_empty()) {
                if plus_options.starts_with('+') || plus_options.contains('=') {
                    // Not an option group (e.g., `++x` or `+foo=bar`);
                    // pass it through unchanged.
                    vec![arg]
                } else {
                    plus_options
                        .chars()
                        .map(|c| format!("+{c}"))
                        .collect::<Vec<_>>()
                }
            } else {
                vec![arg]
            }
        })
        .collect()
}

/// Parses only an option section (already stripped of the command name) for
/// the given command; used by declaration-style builtins whose operands are
/// handled separately from their options.
fn parse_options_only<T: Command>(mut options: Vec<String>) -> Result<T, BuiltinArgParseError> {
    if T::takes_plus_options() {
        options = expand_plus_option_groups(options);
    }

    run_parser::<T>(&options)
}

/// Runs the given command's parser against the provided arguments.
fn run_parser<T: Command>(args: &[String]) -> Result<T, BuiltinArgParseError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_parse_failure)
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
/// implementation defines its parser solely for help/usage information.
/// Arguments are passed directly to the command via `set_declarations`. This
/// is primarily only expected to be used with select builtin commands that
/// wrap other builtins (e.g., "builtin").
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
        Err(e) => return Ok(report_arg_parse_error(&context, &e)),
    };

    call_builtin(command, context).await
}

/// Reports a built-in argument parse error to the appropriate streams and
/// returns the corresponding execution result.
///
/// Help requests are reported to standard output and yield a successful exit
/// code; usage errors are reported to standard error and yield an invalid
/// usage exit code.
fn report_arg_parse_error(
    context: &commands::ExecutionContext<'_, impl extensions::ShellExtensions>,
    e: &BuiltinArgParseError,
) -> results::ExecutionResult {
    if e.help_request {
        let _ = writeln!(context.stdout(), "{}", e.message);
        results::ExecutionResult::success()
    } else {
        let _ = writeln!(context.stderr(), "{}", e.message);
        results::ExecutionExitCode::InvalidUsage.into()
    }
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

    // N.B. The first argument is the command name itself; it is skipped here.
    for arg in args.into_iter().skip(1) {
        match arg {
            CommandArg::String(s) if s.len() > 1 && (s.starts_with('-') || s.starts_with('+')) => {
                options.push(s);
            }
            _ => declarations.push(arg),
        }
    }

    let result = parse_options_only::<T>(options);
    let mut command = match result {
        Ok(command) => command,
        Err(e) => return Ok(report_arg_parse_error(&context, &e)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn split(args: &[&str], shorts: &str) -> (Vec<String>, Vec<String>) {
        split_option_section(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            shorts,
            &[],
        )
    }

    #[test]
    fn test_split_option_section_stops_at_operand() {
        let (options, trailing) = split(&["-n", "hi", "-e"], "");
        assert_eq!(options, ["-n"]);
        assert_eq!(trailing, ["hi", "-e"]);
    }

    #[test]
    fn test_split_option_section_double_dash_is_preserved_in_trailing_args() {
        let (options, trailing) = split(&["-n", "--", "-x"], "");
        assert_eq!(options, ["-n"]);
        assert_eq!(trailing, ["--", "-x"]);

        // A lone trailing `--` is preserved as well.
        let (options, trailing) = split(&["--"], "");
        assert_eq!(options.len(), 0);
        assert_eq!(trailing, ["--"]);
    }

    #[test]
    fn test_split_option_section_value_taking_short_option() {
        // Attached value.
        let (options, trailing) = split(&["-d:", "rest"], "d");
        assert_eq!(options, ["-d:"]);
        assert_eq!(trailing, ["rest"]);

        // Separate, flag-looking value.
        let (options, trailing) = split(&["-d", "-x", "rest"], "d");
        assert_eq!(options, ["-d", "-x"]);
        assert_eq!(trailing, ["rest"]);
    }

    #[test]
    fn test_split_option_section_plus_options() {
        let (options, trailing) = split(&["+v", "+x", "foo"], "");
        assert_eq!(options, ["+v", "+x"]);
        assert_eq!(trailing, ["foo"]);

        // A plus-style option group taking a separate value.
        let (options, trailing) = split(&["+o", "optname", "foo"], "o");
        assert_eq!(options, ["+o", "optname"]);
        assert_eq!(trailing, ["foo"]);
    }

    #[test]
    fn test_split_option_section_dash_is_an_operand() {
        let (options, trailing) = split(&["-n", "-", "more"], "");
        assert_eq!(options, ["-n"]);
        assert_eq!(trailing, ["-", "more"]);
    }

    #[test]
    fn test_expand_plus_option_groups() {
        let args: Vec<String> = ["-a", "+vx", "foo", "+o"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let expanded = expand_plus_option_groups(args);
        assert_eq!(expanded, ["-a", "+v", "+x", "foo", "+o"]);
    }
}
