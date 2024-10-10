//! Infrastructure for shell built-in commands.

use clap::builder::styling;
use clap::Parser;
use futures::future::BoxFuture;

use crate::commands;
use crate::error;
use crate::ExecutionResult;

mod alias;
mod bg;
mod break_;
mod brushinfo;
mod builtin_;
mod cd;
mod colon;
mod command;
mod complete;
mod continue_;
mod declare;
mod dirs;
mod dot;
mod echo;
mod enable;
mod eval;
#[cfg(unix)]
mod exec;
mod exit;
mod export;
mod factory;
mod false_;
mod fg;
mod getopts;
mod help;
mod jobs;
#[cfg(unix)]
mod kill;
mod let_;
mod popd;
mod printf;
mod pushd;
mod pwd;
mod read;
mod return_;
mod set;
mod shift;
mod shopt;
mod test;
mod trap;
mod true_;
mod type_;
#[cfg(unix)]
mod umask;
mod unalias;
mod unimp;
mod unset;
mod wait;

pub(crate) use factory::get_default_builtins;
pub use factory::{builtin, simple_builtin, SimpleCommand};

/// Macro to define a struct that represents a shell built-in flag argument that can be
/// enabled or disabled by specifying an option with a leading '+' or '-' character.
///
/// # Arguments
///
/// - `$struct_name` - The identifier to be used for the struct to define.
/// - `$flag_char` - The character to use as the flag.
/// - `$desc` - The string description of the flag.
#[macro_export]
macro_rules! minus_or_plus_flag_arg {
    ($struct_name:ident, $flag_char:literal, $desc:literal) => {
        #[derive(clap::Parser)]
        pub(crate) struct $struct_name {
            #[arg(short = $flag_char, name = concat!(stringify!($struct_name), "_enable"), action = clap::ArgAction::SetTrue, help = $desc)]
            _enable: bool,
            #[arg(long = concat!("+", $flag_char), name = concat!(stringify!($struct_name), "_disable"), action = clap::ArgAction::SetTrue, hide = true)]
            _disable: bool,
        }

        impl From<$struct_name> for Option<bool> {
            fn from(value: $struct_name) -> Self {
                value.to_bool()
            }
        }

        impl $struct_name {
            #[allow(dead_code)]
            pub fn is_some(&self) -> bool {
                self._enable || self._disable
            }

            pub fn to_bool(&self) -> Option<bool> {
                match (self._enable, self._disable) {
                    (true, false) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                }
            }
        }
    };
}

pub(crate) use minus_or_plus_flag_arg;

/// Result of executing a built-in command.
#[allow(clippy::module_name_repetitions)]
pub struct BuiltinResult {
    /// The exit code from the command.
    pub exit_code: ExitCode,
}

/// Exit codes for built-in commands.
pub enum ExitCode {
    /// The command was successful.
    Success,
    /// The inputs to the command were invalid.
    InvalidUsage,
    /// The command is not implemented.
    Unimplemented,
    /// The command returned a specific custom numerical exit code.
    Custom(u8),
    /// The command is requesting to exit the shell, yielding the given exit code.
    ExitShell(u8),
    /// The command is requesting to return from a function or script, yielding the given exit
    /// code.
    ReturnFromFunctionOrScript(u8),
    /// The command is requesting to continue a loop, identified by the given nesting count.
    ContinueLoop(u8),
    /// The command is requesting to break a loop, identified by the given nesting count.
    BreakLoop(u8),
}

impl From<ExecutionResult> for ExitCode {
    fn from(result: ExecutionResult) -> Self {
        if let Some(count) = result.continue_loop {
            ExitCode::ContinueLoop(count)
        } else if let Some(count) = result.break_loop {
            ExitCode::BreakLoop(count)
        } else if result.return_from_function_or_script {
            ExitCode::ReturnFromFunctionOrScript(result.exit_code)
        } else if result.exit_shell {
            ExitCode::ExitShell(result.exit_code)
        } else if result.exit_code == 0 {
            ExitCode::Success
        } else {
            ExitCode::Custom(result.exit_code)
        }
    }
}

/// Type of a function implementing a built-in command.
///
/// # Arguments
///
/// * The context in which the command is being executed.
/// * The arguments to the command.
pub type CommandExecuteFunc = fn(
    commands::ExecutionContext<'_>,
    Vec<commands::CommandArg>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>>;

/// Type of a function to retrieve help content for a built-in command.
///
/// # Arguments
///
/// * `name` - The name of the command.
/// * `content_type` - The type of content to retrieve.
pub type CommandContentFunc = fn(&str, ContentType) -> Result<String, error::Error>;

/// Trait implemented by built-in shell commands.

pub trait Command: Parser {
    /// Instantiates the built-in command with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments to the command.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        if !Self::takes_plus_options() {
            Self::try_parse_from(args)
        } else {
            // N.B. clap doesn't support named options like '+x'. To work around this, we
            // establish a pattern of renaming them.
            let args = args.into_iter().map(|arg| {
                if arg.starts_with('+') {
                    format!("--{arg}")
                } else {
                    arg
                }
            });

            Self::try_parse_from(args)
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
    fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> impl std::future::Future<Output = Result<ExitCode, error::Error>> + std::marker::Send;

    /// Returns the textual help content associated with the command.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    /// * `content_type` - The type of content to retrieve.
    fn get_content(name: &str, content_type: ContentType) -> Result<String, error::Error> {
        let mut clap_command = Self::command().styles(brush_help_styles());
        clap_command.set_bin_name(name);

        let s = match content_type {
            ContentType::DetailedHelp => clap_command.render_long_help().ansi().to_string(),
            ContentType::ShortUsage => get_builtin_short_usage(name, &clap_command),
            ContentType::ShortDescription => get_builtin_short_description(name, &clap_command),
            ContentType::ManPage => get_builtin_man_page(name, &clap_command)?,
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

/// Encapsulates a registration for a built-in command.
#[derive(Clone)]
pub struct Registration {
    /// Function to execute the builtin.
    pub execute_func: CommandExecuteFunc,

    /// Function to retrieve the builtin's content/help text.
    pub content_func: CommandContentFunc,

    /// Has this registration been disabled?
    pub disabled: bool,

    /// Is the builtin classified as "special" by specification?
    pub special_builtin: bool,

    /// Is this builtin one that takes specially handled declarations?
    pub declaration_builtin: bool,
}

fn get_builtin_man_page(_name: &str, _command: &clap::Command) -> Result<String, error::Error> {
    error::unimp("man page rendering is not yet implemented")
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

/// This function and the [`try_parse_known`] exists to deal with
/// the Clap's limitation of treating `--` like a regular value
/// `https://github.com/clap-rs/clap/issues/5055`
///
/// # Arguments
///
/// * `args` - An Iterator from [`std::env::args`]
///
/// # Returns
///
/// * a parsed struct T from [`clap::Parser::parse_from`]
/// * the remain iterator `args` with `--` and the rest arguments if they present
///   othervise None
///
/// # Examples
/// ```
///    use clap::{builder::styling, Parser};
///    #[derive(Parser)]
///    struct CommandLineArgs {
///       #[clap(allow_hyphen_values = true, num_args=1..)]
///       script_args: Vec<String>,
///    }
///
///    let (mut parsed_args, raw_args) =
///        brush_core::builtins::parse_known::<CommandLineArgs, _>(std::env::args());
///    if raw_args.is_some() {
///        parsed_args.script_args = raw_args.unwrap().collect();
///    }
/// ```
pub fn parse_known<T: Parser, S>(
    args: impl IntoIterator<Item = S>,
) -> (T, Option<impl Iterator<Item = S>>)
where
    S: Into<std::ffi::OsString> + Clone + PartialEq<&'static str>,
{
    let mut args = args.into_iter();
    // the best way to save `--` is to get it out with a side effect while `clap` iterates over the args
    // this way we can be 100% sure that we have '--' and the remaining args
    // and we will iterate only once
    let mut hyphen = None;
    let args_before_hyphen = args.by_ref().take_while(|a| {
        let is_hyphen = *a == "--";
        if is_hyphen {
            hyphen = Some(a.clone());
        }
        !is_hyphen
    });
    let parsed_args = T::parse_from(args_before_hyphen);
    let raw_args = hyphen.map(|hyphen| std::iter::once(hyphen).chain(args));
    (parsed_args, raw_args)
}

/// Similar to [`parse_known`] but with [`clap::Parser::try_parse_from`]
/// This function is used to parse arguments in builtins such as [`crate::builtins::echo::EchoCommand`]
pub fn try_parse_known<T: Parser>(
    args: impl IntoIterator<Item = String>,
) -> Result<(T, Option<impl Iterator<Item = String>>), clap::Error> {
    let mut args = args.into_iter();
    let mut hyphen = None;
    let args_before_hyphen = args.by_ref().take_while(|a| {
        let is_hyphen = a == "--";
        if is_hyphen {
            hyphen = Some(a.clone());
        }
        !is_hyphen
    });
    let parsed_args = T::try_parse_from(args_before_hyphen)?;

    let raw_args = hyphen.map(|hyphen| std::iter::once(hyphen).chain(args));
    Ok((parsed_args, raw_args))
}
