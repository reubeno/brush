#![deny(clippy::all)]
// #![deny(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]

use std::{io::IsTerminal, path::Path};

use anyhow::Result;
use clap::Parser;
use log::error;

#[derive(Parser, Debug)]
#[clap(version, about, disable_help_flag = true, disable_version_flag = true)]
struct CommandLineArgs {
    #[clap(long = "help", action = clap::ArgAction::HelpLong, help = "Display usage information")]
    help: Option<bool>,

    #[clap(long = "version", action = clap::ArgAction::Version, help = "Display shell version")]
    version: Option<bool>,

    #[arg(short = 'c', help = "Execute the provided command and then exit")]
    command: Option<String>,

    #[clap(short = 'i', help = "Run in interactive mode")]
    interactive: bool,

    #[clap(
        short = 'l',
        long = "login",
        help = "Make shell act as if it had been invoked as a login shell"
    )]
    login: bool,

    #[clap(long = "noediting", help = "Don't use readline for input.")]
    no_editing: bool,

    #[clap(
        long = "noprofile",
        help = "Don't process any profile/login files (/etc/profile, ~/.bash_profile, ~/.bash_login, ~/.profile)."
    )]
    no_profile: bool,

    #[clap(
        long = "norc",
        help = "Don't process ~/.bashrc if the shell is interactive."
    )]
    no_rc: bool,

    #[clap(long = "posix", help = "Disable non-POSIX extensions.")]
    posix: bool,

    #[clap(
        short = 'v',
        long = "verbose",
        help = "Print input when it's processed."
    )]
    verbose: bool,

    #[clap(help = "Path to script to execute")]
    script_path: Option<String>,

    #[clap(help = "Arguments for script")]
    script_args: Vec<String>,
}

impl CommandLineArgs {
    pub fn is_interactive(&self) -> bool {
        if self.interactive {
            return true;
        }

        if self.command.is_some() || self.script_path.is_some() {
            return false;
        }

        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return false;
        }

        true
    }
}

fn main() {
    // Initialize logging. Default log level to INFO if not explicitly specified by the env.
    // Keep verbosity on rustyline no more than WARNING, since it otherwise gets quite noisy.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("rustyline", log::LevelFilter::Warn)
        .format_timestamp(None)
        .format_target(false)
        .init();

    let exit_code: u8 = match run(std::env::args().collect()) {
        Ok(code) => code,
        Err(e) => {
            error!("error: {:#}", e);
            1
        }
    };

    std::process::exit(exit_code as i32);
}

fn run(cli_args: Vec<String>) -> Result<u8> {
    let argv0 = if !cli_args.is_empty() {
        Some(cli_args[0].to_owned())
    } else {
        None
    };

    let args = CommandLineArgs::parse_from(cli_args.clone());

    let options = shell::ShellCreateOptions {
        login: args.login || argv0.as_ref().map_or(false, |a0| a0.starts_with('-')),
        interactive: args.is_interactive(),
        no_editing: args.no_editing,
        no_profile: args.no_profile,
        no_rc: args.no_rc,
        posix: args.posix,
        shell_name: argv0.clone(),
        verbose: args.verbose,
    };

    let mut shell = interactive_shell::InteractiveShell::new(&options)?;

    if let Some(command) = args.command {
        // TODO: Use script_path as $0 and remaining args as positional parameters.
        shell.shell.run_string(&command, false)?;
    } else if let Some(script_path) = args.script_path {
        shell
            .shell
            .run_script(Path::new(&script_path), args.script_args.as_slice())?;
    } else {
        shell.run_interactively()?;
    }

    Ok(shell.shell.last_result())
}
