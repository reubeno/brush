//! Types for brush command-line parsing.

use clap::{Parser, builder::styling};
use std::{io::IsTerminal, path::PathBuf};

use crate::{events, productinfo};

const SHORT_DESCRIPTION: &str = "Bo[u]rn[e] RUsty SHell";

const LONG_DESCRIPTION: &str = r"
brush is a Rust-implemented, POSIX-style shell that aims to be compatible with bash.

brush is a work in progress. If you encounter any issues or discrepancies in behavior from bash, please report them at https://github.com/reubeno/brush.
";

const VERSION: &str = const_format::concatcp!(
    productinfo::PRODUCT_VERSION,
    " (",
    productinfo::PRODUCT_GIT_VERSION,
    ")"
);

/// Identifies the input backend to use for the shell.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum InputBackend {
    /// Richest input backend, based on reedline.
    Reedline,
    /// Basic input backend that provides minimal completion support for testing.
    Basic,
    /// Most minimal input backend.
    Minimal,
}

/// Parsed command-line arguments for the brush shell.
#[derive(Parser)]
#[clap(name = productinfo::PRODUCT_NAME,
       version = VERSION,
       about = SHORT_DESCRIPTION,
       long_about = LONG_DESCRIPTION,
       author,
       disable_help_flag = true,
       disable_version_flag = true,
       styles = brush_help_styles())]
pub struct CommandLineArgs {
    /// Display usage information.
    #[clap(long = "help", action = clap::ArgAction::HelpLong)]
    pub help: Option<bool>,

    /// Display shell version.
    #[clap(long = "version", action = clap::ArgAction::Version)]
    pub version: Option<bool>,

    /// Enable noclobber shell option.
    #[arg(short = 'C')]
    pub disallow_overwriting_regular_files_via_output_redirection: bool,

    /// Execute the provided command and then exit.
    #[arg(short = 'c', value_name = "COMMAND")]
    pub command: Option<String>,

    /// Run in interactive mode.
    #[clap(short = 'i')]
    pub interactive: bool,

    /// Make shell act as if it had been invoked as a login shell.
    #[clap(short = 'l', long = "login")]
    pub login: bool,

    /// Do not execute commands.
    #[clap(short = 'n')]
    pub do_not_execute_commands: bool,

    /// Don't use readline for input.
    #[clap(long = "noediting")]
    pub no_editing: bool,

    /// Don't process any profile/login files (`/etc/profile`, `~/.bash_profile`, `~/.bash_login`,
    /// `~/.profile`).
    #[clap(long = "noprofile")]
    pub no_profile: bool,

    /// Don't process "rc" files if the shell is interactive (e.g., `~/.bashrc`, `~/.brushrc`).
    #[clap(long = "norc")]
    pub no_rc: bool,

    /// Don't inherit environment variables from the calling process.
    #[clap(long = "noenv")]
    pub do_not_inherit_env: bool,

    /// Enable option (set -o option).
    #[clap(short = 'o', value_name = "OPTION")]
    pub enabled_options: Vec<String>,

    /// Disable option (set -o option).
    #[clap(long = "+o", hide = true)]
    pub disabled_options: Vec<String>,

    /// Enable shopt option.
    #[clap(short = 'O', value_name = "SHOPT_OPTION")]
    pub enabled_shopt_options: Vec<String>,

    /// Disable shopt option.
    #[clap(long = "+O", hide = true)]
    pub disabled_shopt_options: Vec<String>,

    /// Disable non-POSIX extensions.
    #[clap(long = "posix")]
    pub posix: bool,

    /// Path to the rc file to load in interactive shells (instead of bash.bashrc and ~/.bashrc).
    #[clap(long = "rcfile", alias = "init-file", value_name = "FILE")]
    pub rc_file: Option<PathBuf>,

    /// Read commands from standard input.
    #[clap(short = 's')]
    pub read_commands_from_stdin: bool,

    /// Run in sh compatibility mode.
    #[clap(long = "sh")]
    pub sh_mode: bool,

    /// Run only one command.
    #[clap(short = 't')]
    pub exit_after_one_command: bool,

    /// Print input when it's processed.
    #[clap(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Print commands as they execute.
    #[clap(short = 'x')]
    pub print_commands_and_arguments: bool,

    /// Disable bracketed paste.
    #[clap(long = "disable-bracketed-paste")]
    pub disable_bracketed_paste: bool,

    /// Disable colorized output.
    #[clap(long = "disable-color")]
    pub disable_color: bool,

    /// Enable syntax highlighting (experimental).
    #[clap(long = "enable-highlighting")]
    pub enable_highlighting: bool,

    /// Input backend.
    #[clap(long = "input-backend")]
    pub input_backend: Option<InputBackend>,

    /// Enable debug logging for classes of tracing events.
    #[clap(long = "debug", alias = "log-enable", value_name = "EVENT")]
    pub enabled_debug_events: Vec<events::TraceEvent>,

    /// Disable logging for classes of tracing events (takes same event types as --debug).
    #[clap(
        long = "disable-event",
        alias = "log-disable",
        value_name = "EVENT",
        hide_possible_values = true
    )]
    pub disabled_events: Vec<events::TraceEvent>,

    /// Path and arguments for script to execute (optional).
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    pub script_args: Vec<String>,
}

impl CommandLineArgs {
    /// Returns whether or not the arguments indicate that the shell should run in interactive mode.
    pub fn is_interactive(&self) -> bool {
        // If -i is provided, then that overrides any further consideration; it forces
        // interactive mode.
        if self.interactive {
            return true;
        }

        // If -c or non-option arguments are provided, then we're not in interactive mode.
        if self.command.is_some() || !self.script_args.is_empty() {
            return false;
        }

        // If *either* stdin or stderr is not a terminal, then we're not in interactive mode.
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return false;
        }

        // In all other cases, we assume interactive mode.
        true
    }
}

/// Returns clap styling to be used for command-line help.
#[doc(hidden)]
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
