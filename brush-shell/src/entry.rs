//! Implements the command-line interface for the `brush` shell.

use crate::args::CommandLineArgs;
use crate::args::InputBackendType;
use crate::brushctl::ShellBuilderBrushBuiltinExt as _;
use crate::error_formatter;
use crate::events;
use crate::productinfo;
use brush_builtins::ShellBuilderExt as _;
#[cfg(feature = "experimental-builtins")]
use brush_experimental_builtins::ShellBuilderExt as _;
use brush_interactive::InteractiveShellExt as _;
use std::sync::LazyLock;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[allow(unused_imports, reason = "only used in some configs")]
use std::io::IsTerminal;

static TRACE_EVENT_CONFIG: LazyLock<Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(None)));

// WARN: this implementation shadows `clap::Parser::parse_from` one so it must be defined
// after the `use clap::Parser`
impl CommandLineArgs {
    // Work around clap's limitation handling `--` like a regular value
    // TODO(cmdline): We can safely remove this `impl` after the issue is resolved
    // https://github.com/clap-rs/clap/issues/5055
    // This function takes precedence over [`clap::Parser::parse_from`]
    fn try_parse_from(itr: impl IntoIterator<Item = String>) -> Result<Self, clap::Error> {
        let (mut this, script_args) = brush_core::builtins::try_parse_known::<Self>(itr)?;

        // if we have `--` and unparsed raw args than
        if let Some(args) = script_args {
            this.script_args.extend(args);
        }

        Ok(this)
    }
}

/// Main entry point for the `brush` shell.
pub fn run() {
    //
    // Install panic handlers to clean up on panic.
    //
    install_panic_handlers();

    //
    // Parse args.
    //
    let mut args: Vec<_> = std::env::args().collect();

    // Work around clap's limitations handling +O options.
    for arg in &mut args {
        if arg.starts_with("+O") {
            arg.insert_str(0, "--");
        }
    }

    let parsed_args = match CommandLineArgs::try_parse_from(args.iter().cloned()) {
        Ok(parsed_args) => parsed_args,
        Err(e) => {
            let _ = e.print();

            // Check for whether this is something we'd truly consider fatal. clap returns
            // errors for `--help`, `--version`, etc.
            let exit_code = match e.kind() {
                clap::error::ErrorKind::DisplayVersion => 0,
                clap::error::ErrorKind::DisplayHelp => 0,
                _ => 1,
            };

            std::process::exit(exit_code);
        }
    };

    //
    // Run.
    //
    #[cfg(any(unix, windows))]
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    #[cfg(not(any(unix, windows)))]
    let mut builder = tokio::runtime::Builder::new_current_thread();

    let result = builder
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_async(args, parsed_args));

    let exit_code = match result {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    std::process::exit(i32::from(exit_code));
}

/// Installs panic handlers to report our panic and cleanly exit on panic.
fn install_panic_handlers() {
    //
    // Set up panic handler. On release builds, it will capture panic details to a
    // temporary .toml file and report a human-readable message to the screen.
    //
    human_panic::setup_panic!(
        human_panic::Metadata::new(productinfo::PRODUCT_NAME, productinfo::PRODUCT_VERSION)
            .homepage(env!("CARGO_PKG_HOMEPAGE"))
            .support("please post a GitHub issue at https://github.com/reubeno/brush/issues/new")
    );

    //
    // If stdout is connected to a terminal, then register a new panic handler that
    // resets the terminal and then invokes the previously registered handler. In
    // dev/debug builds, the previously registered handler will be the default
    // handler; in release builds, it will be the one registered by `human_panic`.
    //
    if std::io::stdout().is_terminal() {
        let original_panic_handler = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Best-effort attempt to reset the terminal to defaults.
            let _ = try_reset_terminal_to_defaults();

            // Invoke the original handler
            original_panic_handler(panic_info);
        }));
    }
}

#[cfg(feature = "experimental")]
const DEFAULT_ENABLE_HIGHLIGHTING: bool = true;
#[cfg(not(feature = "experimental"))]
const DEFAULT_ENABLE_HIGHLIGHTING: bool = false;

/// Run the brush shell. Returns the exit code.
///
/// # Arguments
///
/// * `cli_args` - The command-line arguments to the shell, in string form.
/// * `args` - The already-parsed command-line arguments.
#[doc(hidden)]
async fn run_async(
    cli_args: Vec<String>,
    args: CommandLineArgs,
) -> Result<u8, brush_interactive::ShellError> {
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.try_lock().unwrap();
    *event_config = Some(events::TraceEventConfig::init(
        &args.enabled_debug_events,
        &args.disabled_events,
    ));
    drop(event_config);

    // Instantiate an appropriately configured shell and wrap it in an `Arc`.
    let shell = instantiate_shell(&args, cli_args).await?;
    let shell = Arc::new(Mutex::new(shell));

    // Run with the selected input backend. Each branch instantiates the concrete
    // backend type and calls `run_in_shell`, preserving static dispatch.
    let default_backend = get_default_input_backend_type();
    let selected_backend = args.input_backend.unwrap_or(default_backend);

    let highlighting = args
        .enable_highlighting
        .unwrap_or(DEFAULT_ENABLE_HIGHLIGHTING);

    #[allow(unused_variables, reason = "not used when no backend features enabled")]
    let ui_options = brush_interactive::UIOptions::builder()
        .disable_bracketed_paste(args.disable_bracketed_paste)
        .disable_color(args.disable_color)
        .disable_highlighting(!highlighting)
        .build();

    let result = match selected_backend {
        #[cfg(all(feature = "reedline", any(unix, windows)))]
        InputBackendType::Reedline => {
            let mut ui = brush_interactive::ReedlineInputBackend::new(&ui_options, &shell)?;
            run_in_shell(&shell, args.clone(), &mut ui).await
        }
        #[cfg(any(not(feature = "reedline"), not(any(unix, windows))))]
        InputBackendType::Reedline => Err(brush_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "basic")]
        InputBackendType::Basic => {
            let mut ui = brush_interactive::BasicInputBackend;
            run_in_shell(&shell, args.clone(), &mut ui).await
        }
        #[cfg(not(feature = "basic"))]
        InputBackendType::Basic => Err(brush_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "minimal")]
        InputBackendType::Minimal => {
            let mut ui = brush_interactive::MinimalInputBackend;
            run_in_shell(&shell, args.clone(), &mut ui).await
        }
        #[cfg(not(feature = "minimal"))]
        InputBackendType::Minimal => Err(brush_interactive::ShellError::InputBackendNotSupported),
    };

    // Display any error that percolated up.
    let exit_code = match result {
        Ok(code) => code,
        Err(brush_interactive::ShellError::ShellError(e)) => {
            let shell = shell.lock().await;
            let mut stderr = shell.stderr();
            let _ = shell.display_error(&mut stderr, &e).await;
            drop(shell);
            1
        }
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    Ok(exit_code)
}

async fn run_in_shell(
    shell_ref: &brush_interactive::ShellRef,
    args: CommandLineArgs,
    ui: &mut impl brush_interactive::InputBackend,
) -> Result<u8, brush_interactive::ShellError> {
    // If a command was specified via -c, then run that command and then exit.
    if let Some(command) = args.command {
        let mut shell = shell_ref.lock().await;

        shell.start_command_string_mode();

        // Execute the command string.
        let params = shell.default_exec_params();
        let source_info = brush_core::SourceInfo::from("-c");
        let _ = shell.run_string(command, &source_info, &params).await?;

        shell.end_command_string_mode()?;

    // If -s was provided, then read commands from stdin. If there was a script (and optionally
    // args) passed on the command line via positional arguments, then we copy over the
    // parameters but do *not* execute it.
    } else if args.read_commands_from_stdin {
        shell_ref.run_interactively(ui).await?;

    // If a script path was provided, then run the script.
    } else if !args.script_args.is_empty() {
        // The path to a script was provided on the command line; run the script.
        shell_ref
            .lock()
            .await
            .run_script(
                Path::new(&args.script_args[0]),
                args.script_args.iter().skip(1),
            )
            .await?;

    // If we got down here, then we don't have any commands to run. We'll be reading
    // them in from stdin one way or the other.
    } else {
        shell_ref.run_interactively(ui).await?;
    }

    // Make sure to return the last result observed in the shell.
    let result = shell_ref.lock().await.last_exit_status();

    Ok(result)
}

async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: Vec<String>,
) -> Result<brush_core::Shell, brush_interactive::ShellError> {
    #[cfg(feature = "experimental")]
    if let Some(load_file) = &args.load_file {
        return instantiate_shell_from_file(load_file.as_path());
    }

    instantiate_shell_from_args(args, cli_args).await
}

#[cfg(feature = "experimental")]
fn instantiate_shell_from_file(
    file_path: &Path,
) -> Result<brush_core::Shell, brush_interactive::ShellError> {
    let mut shell: brush_core::Shell = serde_json::from_reader(std::fs::File::open(file_path)?)
        .map_err(|e| brush_interactive::ShellError::IoError(std::io::Error::other(e)))?;

    // NOTE: We need to manually register builtins because we can't serialize/deserialize them.
    // TODO(serde): we should consider whether we could/should at least track *which* are enabled.
    let builtin_set = if shell.options.sh_mode {
        brush_builtins::BuiltinSet::ShMode
    } else {
        brush_builtins::BuiltinSet::BashMode
    };

    let builtins = brush_builtins::default_builtins(builtin_set);

    for (builtin_name, builtin) in builtins {
        shell.register_builtin(&builtin_name, builtin);
    }

    Ok(shell)
}

async fn instantiate_shell_from_args(
    args: &CommandLineArgs,
    cli_args: Vec<String>,
) -> Result<brush_core::Shell, brush_interactive::ShellError> {
    // Compute login flag.
    let login = args.login || cli_args.first().is_some_and(|argv0| argv0.starts_with('-'));

    // Compute shell name.
    let shell_name = if args.command.is_some() && !args.script_args.is_empty() {
        Some(args.script_args[0].clone())
    } else if !cli_args.is_empty() {
        Some(cli_args[0].clone())
    } else if args.sh_mode {
        // Simulate having been run as "sh".
        Some(String::from("sh"))
    } else {
        None
    };

    // Compute positional shell arguments.
    let shell_args = if args.command.is_some() {
        Some(args.script_args.iter().skip(1).cloned().collect())
    } else if args.read_commands_from_stdin {
        Some(args.script_args.clone())
    } else {
        None
    };

    // Commands are read from stdin if -s was provided, or if no command was specified (either via
    // -c or as a positional argument).
    let read_commands_from_stdin = (args.read_commands_from_stdin && args.command.is_none())
        || (args.script_args.is_empty() && args.command.is_none());

    let builtin_set = if args.sh_mode {
        brush_builtins::BuiltinSet::ShMode
    } else {
        brush_builtins::BuiltinSet::BashMode
    };

    // Identify the file descriptors to inherit.
    let fds = args
        .inherited_fds
        .iter()
        .filter_map(|&fd| brush_core::sys::fd::try_get_file_for_open_fd(fd).map(|file| (fd, file)))
        .collect();

    // Set up the shell builder with the requested options.
    let shell = brush_core::Shell::builder()
        .disable_options(args.disabled_options.clone())
        .disable_shopt_options(args.disabled_shopt_options.clone())
        .disallow_overwriting_regular_files_via_output_redirection(
            args.disallow_overwriting_regular_files_via_output_redirection,
        )
        .enable_options(args.enabled_options.clone())
        .enable_shopt_options(args.enabled_shopt_options.clone())
        .do_not_execute_commands(args.do_not_execute_commands)
        .exit_after_one_command(args.exit_after_one_command)
        .login(login)
        .interactive(args.is_interactive())
        .command_string_mode(args.command.is_some())
        .no_editing(args.no_editing)
        .no_profile(args.no_profile)
        .no_rc(args.no_rc)
        .maybe_rc_file(args.rc_file.clone())
        .do_not_inherit_env(args.do_not_inherit_env)
        .fds(fds)
        .maybe_shell_args(shell_args)
        .posix(args.posix || args.sh_mode)
        .print_commands_and_arguments(args.print_commands_and_arguments)
        .read_commands_from_stdin(read_commands_from_stdin)
        .maybe_shell_name(shell_name)
        .shell_product_display_str(productinfo::get_product_display_str())
        .sh_mode(args.sh_mode)
        .treat_unset_variables_as_error(args.treat_unset_variables_as_error)
        .verbose(args.verbose)
        .error_formatter(new_error_formatter(args))
        .shell_version(env!("CARGO_PKG_VERSION").to_string());

    // Add builtins.
    let shell = shell.default_builtins(builtin_set).brush_builtins();

    // Add experimental builtins (if enabled).
    #[cfg(feature = "experimental-builtins")]
    let shell = shell.experimental_builtins();

    // Build the shell.
    let shell = shell.build().await?;

    Ok(shell)
}

fn new_error_formatter(
    args: &CommandLineArgs,
) -> Arc<Mutex<dyn brush_core::error::ErrorFormatter>> {
    let formatter = error_formatter::Formatter {
        use_color: !args.disable_color,
    };

    Arc::new(Mutex::new(formatter))
}

fn get_default_input_backend_type() -> InputBackendType {
    #[cfg(any(unix, windows))]
    {
        // If stdin isn't a terminal, then `reedline` doesn't do the right thing
        // (reference: https://github.com/nushell/reedline/issues/509). Switch to
        // the minimal input backend instead for that scenario.
        if std::io::stdin().is_terminal() {
            InputBackendType::Reedline
        } else {
            InputBackendType::Minimal
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        InputBackendType::Minimal
    }
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
}

fn try_reset_terminal_to_defaults() -> Result<(), std::io::Error> {
    #[cfg(any(unix, windows))]
    {
        // Reset the console.
        let exec_result = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::terminal::EnableLineWrap,
            crossterm::style::ResetColor,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show,
            crossterm::cursor::MoveToNextLine(1),
        );

        let raw_result = crossterm::terminal::disable_raw_mode();

        exec_result?;
        raw_result?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn parse_empty_args() -> Result<()> {
        let args = vec!["brush"];
        let args = args.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let parsed_args = CommandLineArgs::try_parse_from(args)?;
        assert_matches!(parsed_args.script_args.as_slice(), []);

        Ok(())
    }

    #[test]
    fn parse_script_and_args() -> Result<()> {
        let args = vec!["brush", "some-script", "-x", "1", "--option"];
        let args = args.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let parsed_args = CommandLineArgs::try_parse_from(args)?;
        assert_eq!(
            parsed_args.script_args,
            ["some-script", "-x", "1", "--option"]
        );

        Ok(())
    }

    #[test]
    fn parse_script_and_args_with_double_dash_in_script_args() -> Result<()> {
        let args = vec!["brush", "some-script", "--"];
        let args = args.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let parsed_args = CommandLineArgs::try_parse_from(args)?;
        assert_eq!(parsed_args.script_args, ["some-script", "--"]);

        Ok(())
    }

    #[test]
    fn parse_unknown_args() {
        let args = vec!["brush", "--unknown-option"];
        let args = args.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let result = CommandLineArgs::try_parse_from(args);
        if let Ok(parsed_args) = &result {
            assert_matches!(parsed_args.script_args.as_slice(), []);
            assert!(result.is_err());
        }
    }
}
