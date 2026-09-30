//! Types for brush command-line parsing.

use std::io::IsTerminal;
use std::path::PathBuf;
use usage::Cli;

use crate::{events, productinfo};

pub(crate) const VERSION: &str = const_format::concatcp!(
    productinfo::PRODUCT_VERSION,
    " (",
    productinfo::PRODUCT_GIT_VERSION,
    ")"
);

/// Identifies the input backend to use for the shell.
#[derive(Clone, Copy, usage::ValueEnum)]
#[usage(rename_all = "lowercase")]
pub enum InputBackendType {
    /// Richest input backend, based on reedline.
    Reedline,
    /// Basic input backend that provides minimal completion support for testing.
    Basic,
    /// Most minimal input backend.
    Minimal,
}

/// A backend name that is not `reedline`, `basic`, or `minimal`.
#[derive(Debug, Clone, Copy)]
pub struct UnknownBackend;

impl std::fmt::Display for UnknownBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown input backend")
    }
}

impl std::error::Error for UnknownBackend {}

impl std::str::FromStr for InputBackendType {
    type Err = UnknownBackend;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "reedline" => Ok(Self::Reedline),
            "basic" => Ok(Self::Basic),
            "minimal" => Ok(Self::Minimal),
            _ => Err(UnknownBackend),
        }
    }
}

/// Parsed command-line arguments for the brush shell.
#[derive(Clone, Cli)]
#[usage(
    name = "brush",
    bin = "brush",
    about = "Bo[u]rn[e] RUsty SHell 🦀 (https://brush.sh)",
    long_about = r"brush is a bash-compatible, Rust-implemented, POSIX-style shell.

brush is distributed under the terms of the MIT license. If you encounter any issues or discrepancies in behavior from bash, please report them at https://github.com/reubeno/brush.

For more information, visit https://brush.sh.",
    disable_help_flag,
    disable_version_flag,
    unknown_flags = "error",
    completion,
    usage = "Usage: brush [FLAGS]... [SCRIPT_PATH [SCRIPT_ARGS]...]\n       brush [FLAGS]... -c COMMAND_STRING [NAME [ARGS]...]"
)]
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

    /// Execute the command given as the first operand and then exit.
    ///
    /// Only an input to parsing: `parse_shell_args` moves the command string into
    /// `command`, which is what everything else consults.
    #[usage(short = 'c', help_heading = "Standard shell options")]
    pub(crate) command_string_mode: bool,

    /// The command string to run, taken from the first operand when `-c` is
    /// given. Not an argument itself (hence `skip`): bash parses options first
    /// and only then takes the command from the first operand, so options may
    /// sit between the two, as in `bash -c -l 'echo hi'`.
    #[usage(skip)]
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
    #[cfg(feature = "experimental")]
    #[usage(
        long = "enable-highlighting",
        help_heading = "User interface options",
        default = "true"
    )]
    pub enable_highlighting: bool,

    /// Enable syntax highlighting in input.
    #[cfg(not(feature = "experimental"))]
    #[usage(
        long = "enable-highlighting",
        help_heading = "User interface options",
        default = "false"
    )]
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
        value_name = "BACKEND",
        value_enum,
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
        value_name = "EVENT",
        value_enum,
        help_heading = "User interface options"
    )]
    pub enabled_debug_events: Vec<events::TraceEvent>,

    /// Disable logging for classes of tracing events (takes same event types as `--debug`).
    #[usage(
        long = "disable-event",
        alias = "log-disable",
        value_name = "EVENT",
        value_enum,
        hide_possible_values = true,
        help_heading = "User interface options"
    )]
    pub disabled_events: Vec<events::TraceEvent>,

    /// Path and arguments for script to execute (optional).
    #[usage(
        trailing_var_arg = true,
        allow_hyphen_values = false,
        value_name = "SCRIPT_PATH [SCRIPT_ARGS]..."
    )]
    pub script_args: Vec<String>,
}

impl CommandLineArgs {
    /// Returns a `CommandLineArgs` with all of its declared default values.
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
        Self::parse_shell_args([String::from("brush")]).expect("parsing defaults should never fail")
    }

    /// Returns whether the shell will read its commands from standard input, as opposed to
    /// running a command given with `-c` or a script named on the command line.
    ///
    /// This is the single source of truth for that question; it decides which shell mode
    /// `entry::run_in_shell` enters, whether `$-` reports `s`, whether the shell can be
    /// interactive, and which input backend gets selected.
    pub const fn will_read_commands_from_stdin(&self) -> bool {
        if self.command.is_some() {
            // -c supplies the command.
            false
        } else if self.read_commands_from_stdin {
            // -s makes any non-option arguments positional parameters, not a script to run.
            true
        } else {
            // Otherwise the first non-option argument, if any, names a script to run.
            self.script_args.is_empty()
        }
    }

    /// Returns whether or not the arguments indicate that the shell should run in interactive mode.
    pub fn is_interactive(&self) -> bool {
        // If -i is provided, then that overrides any further consideration; it forces
        // interactive mode.
        if self.interactive {
            return true;
        }

        // Running a -c command or a named script is not interactive.
        if !self.will_read_commands_from_stdin() {
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

    #[test]
    #[allow(clippy::expect_used)]
    fn test_will_read_commands_from_stdin() {
        // (arguments, whether commands will be read from stdin)
        let cases = [
            (vec![], true),
            (vec!["-i"], true),
            (vec!["-c", "echo hi"], false),
            (vec!["-c", "echo hi", "name"], false),
            // `-i` forces an interactive shell, but `-c` still supplies the commands.
            (vec!["-i", "-c", "echo hi"], false),
            // `-c` is a plain flag; the command string is the first non-option argument.
            (vec!["-cl", "echo hi"], false),
            (vec!["-c", "-l", "echo hi"], false),
            (vec!["script.sh"], false),
            (vec!["-i", "script.sh"], false),
            // `-s` claims the script slot, so trailing words are positional parameters.
            (vec!["-s"], true),
            (vec!["-s", "myarg"], true),
            (vec!["-i", "-s", "myarg"], true),
            (vec!["-si", "myarg"], true),
            // `--` in option position is consumed as the end-of-options marker.
            (vec!["--", "script.sh"], false),
            (vec!["-s", "--", "myarg"], true),
        ];

        for (args, expected) in cases {
            // NOTE: This deliberately goes through `parse_shell_args` and not the derived
            // `parse_from`; only the former applies brush's `--` and `-c` handling, so only
            // the former sees what the shell sees.
            let parsed = CommandLineArgs::parse_shell_args(
                std::iter::once("brush")
                    .chain(args.iter().copied())
                    .map(String::from),
            )
            .expect("arguments should parse");
            assert_eq!(
                parsed.will_read_commands_from_stdin(),
                expected,
                "for arguments: {args:?}"
            );
        }
    }
}
