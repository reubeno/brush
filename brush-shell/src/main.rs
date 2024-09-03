//! Implements the command-line interface for the `brush` shell.

#![deny(missing_docs)]

mod args;
mod brushctl;
mod events;
mod productinfo;

use crate::args::CommandLineArgs;
use clap::Parser;
use std::{path::Path, sync::Arc};

lazy_static::lazy_static! {
    static ref TRACE_EVENT_CONFIG: Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> =
        Arc::new(tokio::sync::Mutex::new(None));
}

/// Main entry point for the `brush` shell.
fn main() {
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
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.try_lock().unwrap();
    *event_config = Some(events::TraceEventConfig::init(&args.enabled_log_events));
    drop(event_config);

    // Instantiate an appropriately configured shell.
    let mut shell = instantiate_shell(&args, cli_args).await?;

    // Handle commands.
    if let Some(command) = args.command {
        // Pass through args.
        if let Some(script_path) = args.script_path {
            shell.shell_mut().shell_name = Some(script_path);
        }
        shell.shell_mut().positional_parameters = args.script_args;

        // Execute the command string.
        let params = shell.shell().default_exec_params();
        shell.shell_mut().run_string(command, &params).await?;
    } else if args.read_commands_from_stdin {
        // We were asked to read commands from stdin; do so, even if there was a script
        // passed on the command line.
        shell.run_interactively().await?;
    } else if let Some(script_path) = args.script_path {
        // The path to a script was provided on the command line; run the script.
        shell
            .shell_mut()
            .run_script(Path::new(&script_path), args.script_args.as_slice())
            .await?;
    } else {
        // In all other cases, run interactively, taking input from stdin.
        shell.run_interactively().await?;
    }

    // Make sure to return the last result observed in the shell.
    Ok(shell.shell().last_result())
}

async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: Vec<String>,
) -> Result<brush_interactive::InteractiveShell, brush_interactive::ShellError> {
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
            posix: args.posix || args.sh_mode,
            print_commands_and_arguments: args.print_commands_and_arguments,
            read_commands_from_stdin,
            shell_name: argv0,
            shell_product_display_str: Some(productinfo::get_product_display_str()),
            sh_mode: args.sh_mode,
            verbose: args.verbose,
        },
        disable_bracketed_paste: args.disable_bracketed_paste,
    };

    // Create the shell.
    let mut shell = brush_interactive::InteractiveShell::new(&options).await?;

    // Register our own built-in(s) with the shell.
    brushctl::register(&mut shell);

    Ok(shell)
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
}
