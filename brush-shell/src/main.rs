//! Implements the command-line interface for the `brush` shell.

mod args;
mod brushctl;
mod events;
mod productinfo;
#[cfg(unix)]
mod rpc;
mod shell_factory;

use crate::args::{CommandLineArgs, InputBackend};
use brush_interactive::InteractiveShell;
use std::sync::LazyLock;
use std::{path::Path, sync::Arc};

#[cfg(any(unix, windows))]
use std::io::IsTerminal;

static TRACE_EVENT_CONFIG: LazyLock<Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(None)));

// WARN: this implementation shadows `clap::Parser::parse_from` one so it must be defined
// after the `use clap::Parser`
impl CommandLineArgs {
    // Work around clap's limitation handling `--` like a regular value
    // TODO: We can safely remove this `impl` after the issue is resolved
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
fn main() {
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
        .block_on(run(args, parsed_args));

    let exit_code = match result {
        Ok(code) => code,
        Err(e) => {
            tracing::error!("error: {:#}", e);
            1
        }
    };

    #[expect(clippy::cast_lossless)]
    std::process::exit(exit_code as i32);
}

/// Installs panic handlers to report our panic and cleanly exit on panic.
fn install_panic_handlers() {
    //
    // Set up panic handler. On release builds, it will capture panic details to a
    // temporary .toml file and report a human-readable message to the screen.
    //
    human_panic::setup_panic!(
        human_panic::Metadata::new(env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"))
            .homepage(env!("CARGO_PKG_HOMEPAGE"))
            .support("please post a GitHub issue at https://github.com/reubeno/brush/issues/new")
    );

    //
    // If stdout is connected to a terminal, then register a new panic handler that
    // resets the terminal and then invokes the previously registered handler. In
    // dev/debug builds, the previously registered handler will be the default
    // handler; in release builds, it will be the one registered by `human_panic`.
    //
    #[cfg(any(unix, windows))]
    if std::io::stdout().is_terminal() {
        let original_panic_handler = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Reset the console.
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::LeaveAlternateScreen,
                crossterm::terminal::EnableLineWrap,
                crossterm::style::ResetColor,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
                crossterm::cursor::Show,
                crossterm::cursor::MoveToNextLine(1),
            );

            let _ = crossterm::terminal::disable_raw_mode();

            // Invoke the original handler
            original_panic_handler(panic_info);
        }));
    }
}

/// Run the brush shell. Returns the exit code.
///
/// # Arguments
///
/// * `cli_args` - The command-line arguments to the shell, in string form.
/// * `args` - The already-parsed command-line arguments.
#[doc(hidden)]
async fn run(
    cli_args: Vec<String>,
    args: CommandLineArgs,
) -> Result<u8, brush_interactive::ShellError> {
    let default_backend = get_default_input_backend();
    let selected_backend = args.input_backend.unwrap_or(default_backend);

    match selected_backend {
        InputBackend::Reedline => {
            run_impl(cli_args, args, shell_factory::ReedlineShellFactory).await
        }
        InputBackend::Basic => run_impl(cli_args, args, shell_factory::BasicShellFactory).await,
        InputBackend::Minimal => run_impl(cli_args, args, shell_factory::MinimalShellFactory).await,
    }
}

/// Run the brush shell. Returns the exit code.
///
/// # Arguments
///
/// * `cli_args` - The command-line arguments to the shell, in string form.
/// * `args` - The already-parsed command-line arguments.
#[doc(hidden)]
async fn run_impl(
    cli_args: Vec<String>,
    args: CommandLineArgs,
    factory: impl shell_factory::ShellFactory + Send + 'static,
) -> Result<u8, brush_interactive::ShellError> {
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.try_lock().unwrap();
    *event_config = Some(events::TraceEventConfig::init(
        &args.enabled_debug_events,
        &args.disabled_events,
    ));
    drop(event_config);

    // Instantiate an appropriately configured shell.
    let mut shell = instantiate_shell(&args, cli_args, factory).await?;

    // If RPC socket specified, serve RPC interface instead of interactive mode.
    #[cfg(unix)]
    if let Some(socket_path) = &args.rpc_socket {
        rpc::serve_rpc(socket_path, &mut shell).await?;
        let result = shell.shell().as_ref().last_result();
        return Ok(result);
    }
    #[cfg(not(unix))]
    if args.rpc_socket.is_some() {
        tracing::error!("RPC socket mode is only supported on Unix platforms");
    }

    // If a command was specified via -c, then run that command and then exit.
    if let Some(command) = args.command {
        // Pass through args.
        if !args.script_args.is_empty() {
            shell.shell_mut().as_mut().shell_name = Some(args.script_args[0].clone());
        }
        shell.shell_mut().as_mut().positional_parameters =
            args.script_args.iter().skip(1).cloned().collect();

        // Execute the command string.
        let params = shell.shell().as_ref().default_exec_params();
        shell
            .shell_mut()
            .as_mut()
            .run_string(command, &params)
            .await?;

    // If -s was provided, then read commands from stdin. If there was a script (and optionally
    // args) passed on the command line via positional arguments, then we copy over the
    // parameters but do *not* execute it.
    } else if args.read_commands_from_stdin {
        if !args.script_args.is_empty() {
            shell
                .shell_mut()
                .as_mut()
                .positional_parameters
                .clone_from(&args.script_args);
        }

        shell.run_interactively().await?;

    // If a script path was provided, then run the script.
    } else if !args.script_args.is_empty() {
        // The path to a script was provided on the command line; run the script.
        shell
            .shell_mut()
            .as_mut()
            .run_script(
                Path::new(&args.script_args[0]),
                args.script_args.iter().skip(1),
            )
            .await?;

    // If we got down here, then we don't have any commands to run. We'll be reading
    // them in from stdin one way or the other.
    } else {
        shell.run_interactively().await?;
    }

    // Make sure to return the last result observed in the shell.
    let result = shell.shell().as_ref().last_result();

    Ok(result)
}

async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: Vec<String>,
    factory: impl shell_factory::ShellFactory + Send + 'static,
) -> Result<impl brush_interactive::InteractiveShell + 'static, brush_interactive::ShellError> {
    let argv0 = if args.sh_mode {
        // Simulate having been run as "sh".
        Some(String::from("sh"))
    } else if !cli_args.is_empty() {
        Some(cli_args[0].clone())
    } else {
        None
    };

    // Commands are read from stdin if -s was provided, or if no command was specified (either via
    // -c or as a positional argument).
    let read_commands_from_stdin = (args.read_commands_from_stdin && args.command.is_none())
        || (args.script_args.is_empty() && args.command.is_none());

    let interactive = args.is_interactive();

    // Compose the options we'll use to create the shell.
    let options = brush_interactive::Options {
        shell: brush_core::CreateOptions {
            disabled_options: args.disabled_options.clone(),
            disabled_shopt_options: args.disabled_shopt_options.clone(),
            disallow_overwriting_regular_files_via_output_redirection: args
                .disallow_overwriting_regular_files_via_output_redirection,
            enabled_options: args.enabled_options.clone(),
            enabled_shopt_options: args.enabled_shopt_options.clone(),
            do_not_execute_commands: args.do_not_execute_commands,
            exit_after_one_command: args.exit_after_one_command,
            login: args.login || argv0.as_ref().is_some_and(|a0| a0.starts_with('-')),
            interactive,
            no_editing: args.no_editing,
            no_profile: args.no_profile,
            no_rc: args.no_rc,
            rc_file: args.rc_file.clone(),
            do_not_inherit_env: args.do_not_inherit_env,
            posix: args.posix || args.sh_mode,
            print_commands_and_arguments: args.print_commands_and_arguments,
            read_commands_from_stdin,
            shell_name: argv0,
            shell_product_display_str: Some(productinfo::get_product_display_str()),
            sh_mode: args.sh_mode,
            verbose: args.verbose,
            max_function_call_depth: None,
            key_bindings: None,
            shell_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
        disable_bracketed_paste: args.disable_bracketed_paste,
        disable_color: args.disable_color,
        disable_highlighting: !args.enable_highlighting,
    };

    // Create the shell.
    let mut shell = factory.create(options).await?;

    // Register our own built-in(s) with the shell.
    brushctl::register(shell.shell_mut().as_mut());

    Ok(shell)
}

fn get_default_input_backend() -> InputBackend {
    #[cfg(any(windows, unix))]
    {
        // If stdin isn't a terminal, then `reedline` doesn't do the right thing
        // (reference: https://github.com/nushell/reedline/issues/509). Switch to
        // the minimal input backend instead for that scenario.
        if std::io::stdin().is_terminal() {
            InputBackend::Reedline
        } else {
            InputBackend::Minimal
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        InputBackend::Minimal
    }
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
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
