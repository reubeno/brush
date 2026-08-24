//! Types for brush command-line parsing.

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::{events, productinfo};

const SHORT_DESCRIPTION: &str = "Bo[u]rn[e] RUsty SHell 🦀 (https://brush.sh)";

const LONG_DESCRIPTION: &str = r"brush is a bash-compatible, Rust-implemented, POSIX-style shell.

brush is distributed under the terms of the MIT license. If you encounter any issues or discrepancies in behavior from bash, please report them at https://github.com/reubeno/brush.

For more information, visit https://brush.sh.";

const VERSION: &str = const_format::concatcp!(
    productinfo::PRODUCT_VERSION,
    " (",
    productinfo::PRODUCT_GIT_VERSION,
    ")"
);

/// Identifies the input backend to use for the shell.
#[derive(Clone, Copy, usage::ValueEnum)]
pub enum InputBackendType {
    /// Richest input backend, based on reedline.
    Reedline,
    /// Basic input backend that provides minimal completion support for testing.
    Basic,
    /// Most minimal input backend.
    Minimal,
}

/// Parsed command-line arguments for the brush shell.
#[derive(Clone, usage::Cli)]
#[usage(bin = "brush",
       name = productinfo::PRODUCT_NAME,
       name_spec = "brush",
       version = VERSION,
       about = SHORT_DESCRIPTION,
       long_about = LONG_DESCRIPTION,
       author = env!("CARGO_PKG_AUTHORS"),
       disable_help_flag,
       disable_version_flag,
       completion,
       unknown_flags = "error",
       args_override_self = false)]
pub struct CommandLineArgs {
    /// Display usage information.
    #[usage(long = "help", action = usage::ArgAction::HelpShort)]
    pub help: bool,

    /// Display shell version.
    #[usage(long = "version", action = usage::ArgAction::Version)]
    pub version: bool,

    /// Path to TOML-based `brush` config file (overrides default location).
    #[usage(
        long = "config",
        value_name = "FILE",
        help_heading = "Configuration options"
    )]
    pub config_file: Option<PathBuf>,

    /// Disable loading of TOML-based `brush` config file.
    #[usage(long = "no-config", help_heading = "Configuration options")]
    pub no_config: bool,

    /// Enable `noclobber` shell option.
    #[usage(short = 'C', help_heading = "Standard shell options")]
    pub disallow_overwriting_regular_files_via_output_redirection: bool,

    /// Execute the provided command and then exit.
    #[usage(
        short = 'c',
        value_name = "COMMAND",
        help_heading = "Standard shell options"
    )]
    pub command: Option<String>,

    /// Enable error-on-exit behavior.
    #[usage(short = 'e', help_heading = "Standard shell options")]
    pub exit_on_nonzero_command_exit: bool,

    /// Disable pathname expansion (also known as filename globbing).
    #[usage(short = 'f', help_heading = "Standard shell options")]
    pub disable_pathname_expansion: bool,

    /// Run in interactive mode.
    #[usage(short = 'i', help_heading = "Standard shell options")]
    pub interactive: bool,

    /// Inherit the specified file descriptors injected by the parent process.
    #[usage(
        long = "inherit-fd",
        value_name = "FD",
        help_heading = "Standard shell options"
    )]
    pub inherited_fds: Vec<i32>,

    /// Make shell act as if it had been invoked as a login shell.
    #[usage(short = 'l', long = "login", help_heading = "Standard shell options")]
    pub login: bool,

    /// Do not execute commands.
    #[usage(short = 'n', help_heading = "Standard shell options")]
    pub do_not_execute_commands: bool,

    /// Don't use readline for input.
    #[usage(long = "noediting", help_heading = "Standard shell options")]
    pub no_editing: bool,

    /// Don't process any profile/login files (`/etc/profile`, `~/.bash_profile`, `~/.bash_login`,
    /// `~/.profile`).
    #[usage(long = "noprofile", help_heading = "Standard shell options")]
    pub no_profile: bool,

    /// Don't process "rc" files if the shell is interactive (e.g., `~/.bashrc`, `~/.brushrc`).
    #[usage(long = "norc", help_heading = "Standard shell options")]
    pub no_rc: bool,

    /// Don't inherit environment variables from the calling process.
    #[usage(long = "noenv", help_heading = "Standard shell options")]
    pub do_not_inherit_env: bool,

    /// Enable option (`set -o` option).
    #[usage(
        short = 'o',
        value_name = "OPTION",
        help_heading = "Standard shell options"
    )]
    pub enabled_options: Vec<String>,

    /// Disable option (`set -o` option).
    #[usage(
        long = "+o",
        value_name = "OPTION",
        hide = true,
        help_heading = "Standard shell options"
    )]
    pub disabled_options: Vec<String>,

    /// Enable `shopt` option.
    #[usage(
        short = 'O',
        value_name = "SHOPT_OPTION",
        help_heading = "Standard shell options"
    )]
    pub enabled_shopt_options: Vec<String>,

    /// Disable `shopt` option.
    #[usage(
        long = "+O",
        value_name = "SHOPT_OPTION",
        hide = true,
        help_heading = "Standard shell options"
    )]
    pub disabled_shopt_options: Vec<String>,

    /// Disable non-POSIX extensions.
    #[usage(long = "posix", help_heading = "Standard shell options")]
    pub posix: bool,

    /// Path to the rc file to load in interactive shells (instead of `bash.bashrc` and
    /// `~/.bashrc`).
    #[usage(
        long = "rcfile",
        alias = "init-file",
        value_name = "FILE",
        help_heading = "Standard shell options"
    )]
    pub rc_file: Option<PathBuf>,

    /// Read commands from standard input.
    #[usage(short = 's', help_heading = "Standard shell options")]
    pub read_commands_from_stdin: bool,

    /// Run in `sh` compatibility mode, as if run as `/bin/sh`.
    #[usage(long = "sh")]
    pub sh_mode: bool,

    /// Run only one command and then exit.
    #[usage(short = 't', help_heading = "Standard shell options")]
    pub exit_after_one_command: bool,

    /// Treat expansion of an unset variable as an error.
    #[usage(short = 'u', help_heading = "Standard shell options")]
    pub treat_unset_variables_as_error: bool,

    /// Print input when it's processed.
    #[usage(short = 'v', long = "verbose", help_heading = "Standard shell options")]
    pub verbose: bool,

    /// Print commands as they execute.
    #[usage(short = 'x', help_heading = "Standard shell options")]
    pub print_commands_and_arguments: bool,

    /// Enable xtrace and configure for the given output file.
    #[usage(
        long = "xtrace-file",
        value_name = "FILE",
        help_heading = "User interface options"
    )]
    pub xtrace_file_path: Option<PathBuf>,

    /// Disable bracketed paste.
    #[usage(
        long = "disable-bracketed-paste",
        help_heading = "User interface options"
    )]
    pub disable_bracketed_paste: bool,

    /// Disable colorized output.
    #[usage(long = "disable-color", help_heading = "User interface options")]
    pub disable_color: bool,

    /// Enable syntax highlighting in input.
    #[cfg_attr(feature = "experimental", usage(default = "true"))]
    #[cfg_attr(not(feature = "experimental"), usage(default = "false"))]
    #[usage(long = "enable-highlighting", help_heading = "User interface options")]
    pub enable_highlighting: bool,

    /// Enable experimental parser (not ready for use).
    #[cfg(feature = "experimental-parser")]
    #[usage(
        long = "experimental-parser",
        help_heading = "*Experimental* options (unstable)"
    )]
    pub experimental_parser: bool,

    /// Enable terminal integration (**experimental**).
    #[usage(
        long = "enable-terminal-integration",
        help_heading = "*Experimental* options (unstable)"
    )]
    pub terminal_shell_integration: bool,

    /// Enable zsh-style preexec/precmd hooks (**experimental**).
    #[usage(
        long = "enable-zsh-hooks",
        help_heading = "*Experimental* options (unstable)"
    )]
    pub zsh_style_hooks: bool,

    /// Input backend.
    #[usage(
        long = "input-backend",
        value_enum,
        value_name = "BACKEND",
        help_heading = "User interface options"
    )]
    pub input_backend: Option<InputBackendType>,

    /// Load state from the given file; the saved state should be in JSON format
    /// and overrides any non-UI command-line options provided.
    #[cfg(feature = "experimental-load")]
    #[usage(
        long = "load",
        value_name = "FILE",
        help_heading = "*Experimental* options (unstable)"
    )]
    pub load_file: Option<PathBuf>,

    /// Enable debug logging for classes of tracing events.
    #[usage(
        long = "debug",
        alias = "log-enable",
        value_enum,
        value_name = "EVENT",
        help_heading = "User interface options"
    )]
    pub enabled_debug_events: Vec<events::TraceEvent>,

    /// Disable logging for classes of tracing events (takes same event types as `--debug`).
    #[usage(
        long = "disable-event",
        alias = "log-disable",
        value_enum,
        value_name = "EVENT",
        help_heading = "User interface options"
    )]
    pub disabled_events: Vec<events::TraceEvent>,

    /// Path and arguments for script to execute (optional).
    #[usage(trailing_var_arg, value_name = "SCRIPT_PATH [SCRIPT_ARGS]...")]
    pub script_args: Vec<String>,
}

brush_core::impl_usage_parse!(CommandLineArgs);

impl CommandLineArgs {
    /// Returns a `CommandLineArgs` with all default values applied.
    ///
    /// This is useful for detecting which CLI arguments were explicitly provided
    /// vs. which retained their default values (e.g., for config file merging).
    #[must_use]
    #[allow(
        clippy::missing_panics_doc,
        reason = "parsing defaults should not panic"
    )]
    pub fn default_values() -> Self {
        // Parse with just the program name to get all defaults.
        // This won't fail because all arguments have defaults or are optional.
        #[allow(clippy::expect_used)]
        Self::parse_from(&[]).expect("parsing defaults should never fail")
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let args = CommandLineArgs::default_values();
        // Verify some basic defaults
        assert!(!args.interactive);
        assert!(!args.login);
        assert!(args.command.is_none());
        assert!(args.script_args.is_empty());
    }
}
