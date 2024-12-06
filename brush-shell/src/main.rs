//! Implements the command-line interface for the `brush` shell.

#![deny(missing_docs)]

mod args;
mod brushctl;
mod events;
mod productinfo;
mod shell_factory;

use crate::args::{CommandLineArgs, InputBackend};
use brush_interactive::InteractiveShell;
use std::{path::Path, sync::Arc};

lazy_static::lazy_static! {
    static ref TRACE_EVENT_CONFIG: Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> =
        Arc::new(tokio::sync::Mutex::new(None));
}

// WARN: this implementation shadows `clap::Parser::parse_from` one so it must be defined
// after the `use clap::Parser`
impl CommandLineArgs {
    // Work around clap's limitation handling `--` like a regular value
    // TODO: We can safely remove this `impl` after the issue is resolved
    // https://github.com/clap-rs/clap/issues/5055
    // This function takes precedence over [`clap::Parser::parse_from`]
    fn parse_from<'a>(itr: impl IntoIterator<Item = &'a String>) -> Self {
        let (mut this, script_args) = brush_core::builtins::parse_known::<CommandLineArgs, _>(itr);
        // if we have `--` and unparsed raw args than
        if let Some(mut args) = script_args {
            // if script_path has not been parsed yet
            // use the first script_args[0] (it is `--`)
            // as script_path
            let first = args.next();
            if this.script_path.is_none() {
                this.script_path = first.cloned();
                this.script_args.extend(args.cloned());
            } else {
                // if script_path already exists, everyone goes into script_args
                this.script_args
                    .extend(first.into_iter().chain(args).cloned());
            }
        }
        this
    }
}

/// Main entry point for the `brush` shell.
fn main() {
    //
    // Set up panic handler. On release builds, it will capture panic details to a
    // temporary .toml file and report a human-readable message to the screen.
    //
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_BIN_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .homepage(env!("CARGO_PKG_HOMEPAGE"))
    .support("please post a GitHub issue at https://github.com/reubeno/brush/issues/new"));

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

    let parsed_args = CommandLineArgs::parse_from(&args);

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

    #[allow(clippy::cast_lossless)]
    std::process::exit(exit_code as i32);
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

    match args.input_backend.as_ref().unwrap_or(&default_backend) {
        InputBackend::Reedline => {
            run_impl(cli_args, args, shell_factory::ReedlineShellFactory).await
        }
        InputBackend::Basic => run_impl(cli_args, args, shell_factory::BasicShellFactory).await,
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
    factory: impl shell_factory::ShellFactory,
) -> Result<u8, brush_interactive::ShellError> {
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.try_lock().unwrap();
    *event_config = Some(events::TraceEventConfig::init(&args.enabled_log_events));
    drop(event_config);

    // Instantiate an appropriately configured shell.
    let mut shell = instantiate_shell(&args, cli_args, factory).await?;

    // Handle commands.
    if let Some(command) = args.command {
        // Pass through args.
        if let Some(script_path) = args.script_path {
            shell.shell_mut().as_mut().shell_name = Some(script_path);
        }
        shell.shell_mut().as_mut().positional_parameters = args.script_args;

        // Execute the command string.
        let params = shell.shell().as_ref().default_exec_params();
        shell
            .shell_mut()
            .as_mut()
            .run_string(command, &params)
            .await?;
    } else if args.read_commands_from_stdin {
        // We were asked to read commands from stdin; do so, even if there was a script
        // passed on the command line.
        shell.run_interactively().await?;
    } else if let Some(script_path) = args.script_path {
        // The path to a script was provided on the command line; run the script.
        shell
            .shell_mut()
            .as_mut()
            .run_script(Path::new(&script_path), args.script_args.as_slice())
            .await?;
    } else {
        // In all other cases, run interactively, taking input from stdin.
        shell.run_interactively().await?;
    }

    // Make sure to return the last result observed in the shell.
    let result = shell.shell().as_ref().last_result();

    Ok(result)
}

async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: Vec<String>,
    factory: impl shell_factory::ShellFactory,
) -> Result<impl brush_interactive::InteractiveShell, brush_interactive::ShellError> {
    let argv0 = if args.sh_mode {
        // Simulate having been run as "sh".
        Some(String::from("sh"))
    } else if !cli_args.is_empty() {
        Some(cli_args[0].clone())
    } else {
        None
    };

    let read_commands_from_stdin = (args.read_commands_from_stdin && args.command.is_none())
        || (args.script_path.is_none() && args.command.is_none());
    let interactive = args.is_interactive();

    // Compose the options we'll use to create the shell.
    let options = brush_interactive::Options {
        shell: brush_core::CreateOptions {
            disabled_shopt_options: args.disabled_shopt_options.clone(),
            enabled_shopt_options: args.enabled_shopt_options.clone(),
            do_not_execute_commands: args.do_not_execute_commands,
            login: args.login || argv0.as_ref().map_or(false, |a0| a0.starts_with('-')),
            interactive,
            no_editing: args.no_editing,
            no_profile: args.no_profile,
            no_rc: args.no_rc,
            do_not_inherit_env: args.do_not_inherit_env,
            posix: args.posix || args.sh_mode,
            print_commands_and_arguments: args.print_commands_and_arguments,
            read_commands_from_stdin,
            shell_name: argv0,
            shell_product_display_str: Some(productinfo::get_product_display_str()),
            sh_mode: args.sh_mode,
            verbose: args.verbose,
            max_function_call_depth: None,
        },
        disable_bracketed_paste: args.disable_bracketed_paste,
        disable_color: args.disable_color,
        disable_highlighting: !args.enable_highlighting,
    };

    // Create the shell.
    let mut shell = factory.create(&options).await?;

    // Register our own built-in(s) with the shell.
    brushctl::register(shell.shell_mut().as_mut());

    Ok(shell)
}

fn get_default_input_backend() -> InputBackend {
    #[cfg(any(windows, unix))]
    {
        InputBackend::Reedline
    }
    #[cfg(not(any(windows, unix)))]
    {
        InputBackend::Basic
    }
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
}
