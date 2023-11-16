use anyhow::Result;
use clap::{Parser, Subcommand};
use log::error;

#[derive(Parser, Debug)]
#[clap(version, about)]
struct CommandLineArgs {
    #[clap(subcommand)]
    command: CommandLineCommand,
}

#[derive(Debug, Subcommand)]
enum CommandLineCommand {
    /// Use as a standard shell.
    Shell(ShellCommandArgs),
}

#[derive(Parser, Debug)]
struct ShellCommandArgs {
    #[arg(short = 'c')]
    command: Option<String>,
}

fn main() {
    // Initialize logging. Default log level to INFO if not explicitly specified by the env.
    // Keep verbosity on rustyline no more than WARNING, since it otherwise gets quite noisy.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("rustyline", log::LevelFilter::Warn)
        .format_timestamp(None)
        .format_target(false)
        .init();

    let exit_code: i32 = match run(std::env::args()) {
        Ok(code) => code,
        Err(e) => {
            error!("error: {:#}", e);
            1
        }
    };

    std::process::exit(exit_code);
}

fn run(cli_args: impl Iterator<Item = String>) -> Result<i32> {
    let parsed_args = CommandLineArgs::parse_from(cli_args);

    //
    // TODO: Look for '-' prefix in argv[0] (or -l perhaps) to indicate login shell.
    //
    let shell_options = shell::ShellOptions {
        login: false,
        interactive: true,
    };

    let mut shell = shell::Shell::new(&shell_options)?;

    match parsed_args.command {
        CommandLineCommand::Shell(cmd_args) => {
            if let Some(command) = cmd_args.command {
                shell.run_string(&command)?;
            } else {
                shell.run_interactively()?;
            }
        }
    }

    Ok(shell.last_result())
}
