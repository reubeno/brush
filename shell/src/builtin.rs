use clap::builder::styling;
use clap::Parser;
use futures::future::BoxFuture;

use crate::commands::CommandArg;
use crate::context;
use crate::error;
use crate::ExecutionResult;

/// Macro to define a struct that represents a shell built-in flag argument that can be
/// enabled or disabled by specifying an option with a leading '+' or '-' character.
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
    pub exit_code: BuiltinExitCode,
}

/// Exit codes for built-in commands.
#[allow(clippy::module_name_repetitions)]
pub enum BuiltinExitCode {
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
    /// The command is requesting to return from a function or script, yielding the given exit code.
    ReturnFromFunctionOrScript(u8),
    /// The command is requesting to continue a loop, identified by the given nesting count.
    ContinueLoop(u8),
    /// The command is requesting to break a loop, identified by the given nesting count.
    BreakLoop(u8),
}

impl From<ExecutionResult> for BuiltinExitCode {
    fn from(result: ExecutionResult) -> Self {
        if let Some(count) = result.continue_loop {
            BuiltinExitCode::ContinueLoop(count)
        } else if let Some(count) = result.break_loop {
            BuiltinExitCode::BreakLoop(count)
        } else if result.return_from_function_or_script {
            BuiltinExitCode::ReturnFromFunctionOrScript(result.exit_code)
        } else if result.exit_shell {
            BuiltinExitCode::ExitShell(result.exit_code)
        } else if result.exit_code == 0 {
            BuiltinExitCode::Success
        } else {
            BuiltinExitCode::Custom(result.exit_code)
        }
    }
}

/// Type of a function implementing a built-in command.
///
/// # Arguments
///
/// * The context in which the command is being executed.
/// * The arguments to the command.
#[allow(clippy::module_name_repetitions)]
pub type BuiltinCommandExecuteFunc = fn(
    context::CommandExecutionContext<'_>,
    Vec<CommandArg>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>>;

/// Trait implemented by built-in shell commands.
#[allow(clippy::module_name_repetitions)]
#[async_trait::async_trait]
pub trait BuiltinCommand: Parser {
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
    async fn execute(
        &self,
        context: context::CommandExecutionContext<'_>,
    ) -> Result<BuiltinExitCode, error::Error>;

    /// Returns the textual help content associated with the command.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    /// * `content_type` - The type of content to retrieve.
    fn get_content(name: &str, content_type: BuiltinContentType) -> String {
        let mut clap_command = Self::command().styles(brush_help_styles());
        clap_command.set_bin_name(name);

        match content_type {
            BuiltinContentType::DetailedHelp => clap_command.render_long_help().ansi().to_string(),
            BuiltinContentType::ShortUsage => get_builtin_short_usage(name, &clap_command),
            BuiltinContentType::ShortDescription => {
                get_builtin_short_description(name, &clap_command)
            }
        }
    }
}

/// Trait implemented by built-in shell commands that take specially handled declarations
/// as arguments.
#[allow(clippy::module_name_repetitions)]
#[async_trait::async_trait]
pub trait BuiltinDeclarationCommand: BuiltinCommand {
    /// Stores the declarations within the command instance.
    ///
    /// # Arguments
    ///
    /// * `declarations` - The declarations to store.
    fn set_declarations(&mut self, declarations: Vec<CommandArg>);
}

/// Type of help content, typically associated with a built-in command.
#[allow(clippy::module_name_repetitions)]
pub enum BuiltinContentType {
    /// Detailed help content for the command.
    DetailedHelp,
    /// Short usage information for the command.
    ShortUsage,
    /// Short description for the command.
    ShortDescription,
}

/// Encapsulates a registration for a built-in command.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct BuiltinRegistration {
    /// Function to execute the builtin.
    pub execute_func: BuiltinCommandExecuteFunc,

    /// Function to retrieve the builtin's content/help text.
    pub content_func: fn(&str, BuiltinContentType) -> String,

    /// Has this registration been disabled?
    pub disabled: bool,

    /// Is the builtin classified as "special" by specification?
    pub special_builtin: bool,

    /// Is this builtin one that takes specially handled declarations?
    pub declaration_builtin: bool,
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
