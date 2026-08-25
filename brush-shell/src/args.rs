//! Types for brush command-line parsing.

use crate::{events, productinfo};
use bpaf::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr;

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
#[derive(Clone, Copy, Debug)]
pub enum InputBackendType {
    /// Richest input backend, based on reedline.
    Reedline,
    /// Basic input backend that provides minimal completion support for testing.
    Basic,
    /// Most minimal input backend.
    Minimal,
}

impl FromStr for InputBackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "reedline" => Ok(Self::Reedline),
            "basic" => Ok(Self::Basic),
            "minimal" => Ok(Self::Minimal),
            _ => Err(format!(
                "invalid input backend: `{s}` (expected one of reedline, basic, minimal)"
            )),
        }
    }
}

/// Parsed command-line arguments for the brush shell.
#[derive(Clone, Debug, Default)]
pub struct CommandLineArgs {
    /// Path to TOML-based `brush` config file (overrides default location).
    pub config_file: Option<PathBuf>,

    /// Disable loading of TOML-based `brush` config file.
    pub no_config: bool,

    /// Enable `noclobber` shell option.
    pub disallow_overwriting_regular_files_via_output_redirection: bool,

    /// Execute the provided command and then exit.
    pub command: Option<String>,

    /// Enable error-on-exit behavior.
    pub exit_on_nonzero_command_exit: bool,

    /// Disable pathname expansion (also known as filename globbing).
    pub disable_pathname_expansion: bool,

    /// Run in interactive mode.
    pub interactive: bool,

    /// Inherit the specified file descriptors injected by the parent process.
    pub inherited_fds: Vec<i32>,

    /// Make shell act as if it had been invoked as a login shell.
    pub login: Option<bool>,

    /// Do not execute commands.
    pub do_not_execute_commands: bool,

    /// Don't use readline for input.
    pub no_editing: bool,

    /// Don't process any profile/login files (`/etc/profile`, `~/.bash_profile`, `~/.bash_login`,
    /// `~/.profile`).
    pub no_profile: bool,

    /// Don't process "rc" files if the shell is interactive (e.g., `~/.bashrc`, `~/.brushrc`).
    pub no_rc: bool,

    /// Don't inherit environment variables from the calling process.
    pub do_not_inherit_env: bool,

    /// Enable option (`set -o` option).
    pub enabled_options: Vec<String>,

    /// Disable option (`set -o` option).
    pub disabled_options: Vec<String>,

    /// Enable `shopt` option.
    pub enabled_shopt_options: Vec<String>,

    /// Disable `shopt` option.
    pub disabled_shopt_options: Vec<String>,

    /// Disable non-POSIX extensions.
    pub posix: bool,

    /// Path to the rc file to load in interactive shells (instead of `bash.bashrc` and
    /// `~/.bashrc`).
    pub rc_file: Option<PathBuf>,

    /// Read commands from standard input.
    pub read_commands_from_stdin: bool,

    /// Run in `sh` compatibility mode, as if run as `/bin/sh`.
    pub sh_mode: bool,

    /// Run only one command and then exit.
    pub exit_after_one_command: bool,

    /// Treat expansion of an unset variable as an error.
    pub treat_unset_variables_as_error: bool,

    /// Print input when it's processed.
    pub verbose: Option<bool>,

    /// Print commands as they execute.
    pub print_commands_and_arguments: bool,

    /// Enable xtrace and configure for the given output file.
    pub xtrace_file_path: Option<PathBuf>,

    /// Disable bracketed paste.
    pub disable_bracketed_paste: bool,

    /// Disable colorized output.
    pub disable_color: bool,

    /// Enable syntax highlighting in input.
    pub enable_highlighting: bool,

    /// Enable experimental parser (not ready for use).
    pub experimental_parser: bool,

    /// Enable terminal integration (**experimental**).
    pub terminal_shell_integration: bool,

    /// Enable zsh-style preexec/precmd hooks (**experimental**).
    pub zsh_style_hooks: bool,

    /// Input backend.
    pub input_backend: Option<InputBackendType>,

    /// Load state from the given file; the saved state should be in JSON format
    /// and overrides any non-UI command-line options provided.
    pub load_file: Option<PathBuf>,

    /// Enable debug logging for classes of tracing events.
    pub enabled_debug_events: Vec<events::TraceEvent>,

    /// Disable logging for classes of tracing events (takes same event types as `--debug`).
    pub disabled_events: Vec<events::TraceEvent>,

    /// Path and arguments for script to execute (optional).
    pub script_args: Vec<String>,
}

/// If the `-c` group's command string is itself `--`, attaches it to the flag
/// (`-c=--`) so the parser does not mistake it for an option separator. Any
/// leading boolean characters in a combined group are split out into their own
/// flags first.
fn merge_dash_dash_value(options: &mut Vec<String>, c_idx: usize, has_value: bool) {
    if has_value && options.len() == c_idx + 2 && options.last().map(String::as_str) == Some("--") {
        let _ = options.pop();
        let flag = options.pop().unwrap_or_else(|| String::from("-c"));
        if let Some(group) = flag.strip_prefix('-').filter(|g| !g.is_empty()) {
            for c in group.chars().take(group.chars().count() - 1) {
                options.push(format!("-{c}"));
            }
        }
        options.push(String::from("-c=--"));
    }
}

/// Short options that take a separate value token; used when deciding where
/// the option section of the command line ends.
const VALUE_TAKING_SHORT_OPTIONS: &str = "coO";

/// Boolean short options; used to detect `-c` at the tail end of a combined
/// group of options.
const BOOLEAN_SHORT_OPTIONS: &str = "Cefilnstuvx";

/// Long options that take a separate value token; used when deciding where
/// the option section of the command line ends.
const VALUE_TAKING_LONG_OPTIONS: &[&str] = &[
    "--config",
    "--inherit-fd",
    "--rcfile",
    "--init-file",
    "--xtrace-file",
    "--input-backend",
    "--load",
    "--debug",
    "--log-enable",
    "--disable-event",
    "--log-disable",
];

impl CommandLineArgs {
    /// Returns a parser for the brush shell's command-line arguments.
    ///
    /// N.B. Only the leading option section is interpreted here; any script
    /// path and arguments are captured verbatim by [`CommandLineArgs::try_parse_from`].
    #[must_use]
    #[expect(clippy::too_many_lines, reason = "one block per command-line option")]
    pub fn parser() -> impl Parser<Self> {
        let config_file = long_config("config")
            .help("Path to TOML-based `brush` config file (overrides default location).")
            .optional();
        let no_config = long_flag(
            "no-config",
            "Disable loading of TOML-based `brush` config file.",
        );
        let disallow_overwriting_regular_files_via_output_redirection = bpaf::short('C')
            .help("Enable `noclobber` shell option.")
            .switch();
        let command = bpaf::short('c')
            .help("Execute the provided command and then exit.")
            .argument::<String>("COMMAND")
            .optional();
        let exit_on_nonzero_command_exit = bpaf::short('e')
            .help("Enable error-on-exit behavior.")
            .switch();
        let disable_pathname_expansion = bpaf::short('f')
            .help("Disable pathname expansion (also known as filename globbing).")
            .switch();
        let interactive = bpaf::short('i').help("Run in interactive mode.").switch();
        let inherit_fd = long_option("inherit-fd")
            .help("Inherit the specified file descriptors injected by the parent process.")
            .argument::<i32>("FD");

        let login = bpaf::short('l')
            .long("login")
            .help("Make shell act as if it had been invoked as a login shell.")
            .req_flag(())
            .map(|(): ()| Some(true))
            .fallback(None);
        let do_not_execute_commands = bpaf::short('n').help("Do not execute commands.").switch();
        let no_editing = long_flag("noediting", "Don't use readline for input.");
        let no_profile = long_flag(
            "noprofile",
            "Don't process any profile/login files (`/etc/profile`, `~/.bash_profile`, `~/.bash_login`, `~/.profile`).",
        );
        let no_rc = long_flag(
            "norc",
            "Don't process \"rc\" files if the shell is interactive (e.g., `~/.bashrc`, `~/.brushrc`).",
        );
        let do_not_inherit_env = long_flag(
            "noenv",
            "Don't inherit environment variables from the calling process.",
        );

        let enabled_options = repeated_value(
            bpaf::short('o'),
            "OPTION",
            "Enable option (`set -o` option)",
        );
        let enabled_shopt_options =
            repeated_value(bpaf::short('O'), "SHOPT_OPTION", "Enable `shopt` option.");
        let posix = long_flag("posix", "Disable non-POSIX extensions.");
        let rc_file = long_option("rcfile")
            .long("init-file")
            .help("Path to the rc file to load in interactive shells.")
            .argument::<PathBuf>("FILE")
            .optional();
        let read_commands_from_stdin = bpaf::short('s')
            .help("Read commands from standard input.")
            .switch();

        let sh_mode = bpaf::long("sh")
            .help("Run in `sh` compatibility mode, as if run as `/bin/sh`.")
            .switch();
        let exit_after_one_command = bpaf::short('t')
            .help("Run only one command and then exit.")
            .switch();
        let treat_unset_variables_as_error = bpaf::short('u')
            .help("Treat expansion of an unset variable as an error.")
            .switch();
        let verbose = bpaf::short('v')
            .long("verbose")
            .help("Print input when it's processed.")
            .req_flag(())
            .map(|(): ()| Some(true))
            .fallback(None);
        let print_commands_and_arguments = bpaf::short('x')
            .help("Print commands as they execute.")
            .switch();

        let xtrace_file_path = long_option("xtrace-file")
            .help("Enable xtrace and configure for the given output file.")
            .argument::<PathBuf>("FILE")
            .optional();

        let disable_bracketed_paste =
            long_flag("disable-bracketed-paste", "Disable bracketed paste.");
        let disable_color = long_flag("disable-color", "Disable colorized output.");
        let enable_highlighting = bpaf::long("enable-highlighting")
            .help("Enable syntax highlighting in input.")
            .switch()
            .fallback(crate::entry::DEFAULT_ENABLE_HIGHLIGHTING);

        #[cfg(feature = "experimental-parser")]
        let experimental_parser = bpaf::long("experimental-parser")
            .help("Enable experimental parser (not ready for use).")
            .switch();
        #[cfg(not(feature = "experimental-parser"))]
        let experimental_parser = pure_default(false);

        let terminal_shell_integration = bpaf::long("enable-terminal-integration")
            .help("Enable terminal integration (**experimental**).")
            .switch();
        let zsh_style_hooks = bpaf::long("enable-zsh-hooks")
            .help("Enable zsh-style preexec/precmd hooks (**experimental**).")
            .switch();

        let input_backend = long_option("input-backend")
            .argument::<InputBackendType>("BACKEND")
            .help("Input backend.")
            .optional();

        #[cfg(feature = "experimental-load")]
        let load_file = bpaf::long("load")
            .help("Load state from the given file; the saved state should be in JSON format and overrides any non-UI command-line options provided.")
            .argument::<PathBuf>("FILE")
            .optional();
        #[cfg(not(feature = "experimental-load"))]
        let load_file = pure_default(None::<PathBuf>);

        let enabled_debug_events = long_option("debug")
            .long("log-enable")
            .help("Enable debug logging for classes of tracing events.")
            .argument::<events::TraceEvent>("EVENT")
            .many()
            .fallback(Vec::new());
        let disabled_events = long_option("disable-event")
            .long("log-disable")
            .help("Disable logging for classes of tracing events.")
            .argument::<events::TraceEvent>("EVENT")
            .many()
            .fallback(Vec::new());

        let inherited_fds = inherit_fd.many().fallback(Vec::new());

        // N.B. These use `any`/`literal`-based parsers, which bpaf requires to
        // be positioned to the right of all other named options.
        let disabled_options = plus_repeated_value("+o", "Disable option (`set -o` option).");
        let disabled_shopt_options = plus_repeated_value("+O", "Disable `shopt` option.");

        let script_args = pure_default(Vec::new());

        bpaf::construct!(CommandLineArgs {
            config_file,
            no_config,
            disallow_overwriting_regular_files_via_output_redirection,
            command,
            exit_on_nonzero_command_exit,
            disable_pathname_expansion,
            interactive,
            inherited_fds,
            login,
            do_not_execute_commands,
            no_editing,
            no_profile,
            no_rc,
            do_not_inherit_env,
            enabled_options,
            enabled_shopt_options,
            posix,
            rc_file,
            read_commands_from_stdin,
            sh_mode,
            exit_after_one_command,
            treat_unset_variables_as_error,
            verbose,
            print_commands_and_arguments,
            xtrace_file_path,
            disable_bracketed_paste,
            disable_color,
            enable_highlighting,
            experimental_parser,
            terminal_shell_integration,
            zsh_style_hooks,
            input_backend,
            load_file,
            enabled_debug_events,
            disabled_events,
            disabled_options,
            disabled_shopt_options,
            script_args,
        })
    }

    /// Returns a `CommandLineArgs` with all default values.
    ///
    /// This is useful for detecting which CLI arguments were explicitly provided
    /// vs. which retained their default values (e.g., for config file merging).
    /// # Panics
    ///
    /// Panics if the default arguments fail to parse, which should be
    /// impossible.
    #[must_use]
    pub fn default_values() -> Self {
        #[expect(clippy::expect_used, reason = "parsing defaults should not panic")]
        Self::try_parse_from(["brush"]).expect("parsing defaults should never fail")
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

    /// Returns the shell's argument parser wrapped with standard help/version
    /// handling.
    #[must_use]
    pub fn option_parser() -> bpaf::OptionParser<Self> {
        Self::parser()
            .to_options()
            .version(VERSION)
            .descr(LONG_DESCRIPTION)
    }

    /// Parses the brush shell's command-line arguments from the given list.
    ///
    /// This is a bash-faithful interpretation of the command line:
    ///
    /// * Options are parsed up to the first operand or `--`; everything after
    ///   that becomes `script_args` verbatim.
    /// * A `--` immediately following `-c` (or a combined group ending in `-c`)
    ///   acts as an option terminator, with the command string taken from the
    ///   next argument.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments, including the program name.
    pub fn try_parse_from<S: Into<String>>(
        args: impl IntoIterator<Item = S>,
    ) -> Result<Self, bpaf::ParseFailure> {
        let mut args: Vec<String> = args.into_iter().map(Into::into).collect();
        if !args.is_empty() {
            args.remove(0); // program name
        }

        // In bash, once `-c` consumes its command string, all remaining
        // arguments become positional script arguments verbatim; notably a
        // following `--` becomes `$0` rather than acting as an option
        // terminator. Handle that by ending the option section right after
        // the `-c` value when a pending `-c` is present.
        // A `--` directly following the `-c` group is an option terminator:
        // the command string is taken from the next argument.
        if let Some(dd_idx) = args.iter().position(|a| a == "--") {
            if dd_idx > 0 && pending_c_group(&args[dd_idx - 1]) {
                args.remove(dd_idx);
                let c_idx = dd_idx - 1;
                let has_value = c_idx + 1 < args.len();

                let mut options: Vec<String> = args[..=(c_idx + 1).min(args.len() - 1)].to_vec();
                let trailing: Vec<String> = if has_value {
                    args[c_idx + 2..].to_vec()
                } else {
                    Vec::new()
                };

                merge_dash_dash_value(&mut options, c_idx, has_value);
                return finish_parsing(&options, trailing);
            }
        }

        let first_dd = args.iter().position(|a| a == "--");
        let c_candidate = args
            .iter()
            .take(first_dd.unwrap_or(args.len()))
            .rposition(|a| pending_c_group(a));

        if let Some(c_idx) = c_candidate {
            let has_value = c_idx + 1 < args.len();

            // Include the `-c` group and its value in the option section.
            let mut options: Vec<String> = args[..=(c_idx + 1).min(args.len() - 1)].to_vec();
            let trailing: Vec<String> = if has_value {
                args[c_idx + 2..].to_vec()
            } else {
                Vec::new()
            };

            merge_dash_dash_value(&mut options, c_idx, has_value);

            return finish_parsing(&options, trailing);
        }

        let (options, trailing) = brush_core::builtins::split_option_section(
            &args,
            VALUE_TAKING_SHORT_OPTIONS,
            VALUE_TAKING_LONG_OPTIONS,
        );

        finish_parsing(&options, trailing)
    }
}

fn finish_parsing(
    options: &[String],
    trailing: Vec<String>,
) -> Result<CommandLineArgs, bpaf::ParseFailure> {
    let mut parsed = CommandLineArgs::option_parser().run_inner(options)?;

    parsed.script_args = trailing;

    Ok(parsed)
}

/// Returns whether `arg` is `-c` or a combined short-flag group ending in `c`
/// (like `-ec`) where all preceding characters are boolean flags.
fn pending_c_group(arg: &str) -> bool {
    let Some(flags) = arg.strip_prefix('-') else {
        return false;
    };
    let Some(preceding) = flags.strip_suffix('c') else {
        return false;
    };
    preceding
        .chars()
        .all(|ch| BOOLEAN_SHORT_OPTIONS.contains(ch))
}

/// A hidden boolean flag with the given long name.
fn long_flag(name: &'static str, help: &'static str) -> impl Parser<bool> {
    bpaf::long(name).help(help).switch()
}

/// A long option with the given name.
fn long_option(name: &'static str) -> bpaf::parsers::NamedArg {
    bpaf::long(name)
}

/// Like [`long_option`] but named `--config`.
fn long_config(name: &'static str) -> bpaf::parsers::ParseArgument<PathBuf> {
    long_option(name).argument::<PathBuf>("FILE")
}

/// A repeatable value-taking option attached to the given named argument.
fn repeated_value(
    arg: bpaf::parsers::NamedArg,
    meta: &'static str,
    help: &'static str,
) -> impl Parser<Vec<String>> {
    arg.help(help)
        .argument::<String>(meta)
        .many()
        .fallback(Vec::new())
}

/// A repeatable plus-style option (e.g., `+o OPTION`) that disables something.
fn plus_repeated_value(plus_form: &'static str, help: &'static str) -> impl Parser<Vec<String>> {
    let tag = bpaf::literal(plus_form).help(help);
    let value = bpaf::any::<String, String, _>("OPTION", Some);
    bpaf::construct!(tag, value)
        .adjacent()
        .many()
        .map(|pairs| pairs.into_iter().map(|((), v)| v).collect())
}

/// A parser that always succeeds with the given value without consuming anything.
fn pure_default<T: Clone + 'static>(value: T) -> impl Parser<T> {
    bpaf::pure(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let args = CommandLineArgs::try_parse_from(["brush"]).unwrap();
        assert!(!args.interactive);
        assert_eq!(args.login, None);
        assert!(args.command.is_none());
        assert!(args.script_args.is_empty());
    }

    #[test]
    fn parse_script_and_args() {
        let parsed_args =
            CommandLineArgs::try_parse_from(["brush", "some-script", "-x", "1", "--option"])
                .unwrap();
        assert_eq!(
            parsed_args.script_args,
            ["some-script", "-x", "1", "--option"]
        );
    }

    #[test]
    fn parse_unknown_args() {
        let result = CommandLineArgs::try_parse_from(["brush", "--unknown-option"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_c_with_double_dash_separator() {
        let parsed_args =
            CommandLineArgs::try_parse_from(["brush", "-c", "--", "echo hello", "arg0"]).unwrap();
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.script_args, ["arg0"]);
    }

    #[test]
    fn parse_c_with_double_dash_no_command() {
        assert!(CommandLineArgs::try_parse_from(["brush", "-c", "--"]).is_err());
    }

    #[test]
    fn parse_ec_with_double_dash_separator() {
        let parsed_args =
            CommandLineArgs::try_parse_from(["brush", "-ec", "--", "echo hello", "arg0"]).unwrap();
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["arg0"]);
    }

    #[test]
    fn parse_o_with_double_dash_is_error() {
        // bash's -o consumes -- as its literal value (invalid option name), so
        // this must not be treated as a terminator for -o.
        let result = CommandLineArgs::try_parse_from(["brush", "-o", "--"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_bool_flag_before_double_dash_not_transformed() {
        // -e is a boolean flag, not -c. The -- terminates options; everything
        // after becomes positional (including -c).
        let parsed_args =
            CommandLineArgs::try_parse_from(["brush", "-e", "--", "-c", "echo"]).unwrap();
        assert!(parsed_args.command.is_none());
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["--", "-c", "echo"]);
    }
}
