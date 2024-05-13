use std::{collections::HashSet, io::IsTerminal, path::Path};

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Parser)]
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

    #[clap(short = 's', help = "Read commands from standard input.")]
    read_commands_from_stdin: bool,

    #[clap(long = "sh", help = "Run in sh compatibility mode.")]
    sh_mode: bool,

    #[clap(
        short = 'v',
        long = "verbose",
        help = "Print input when it's processed."
    )]
    verbose: bool,

    #[clap(short = 'x', help = "Print commands as they execute.")]
    print_commands_and_arguments: bool,

    #[clap(long = "disable-bracketed-paste", help = "Disable bracketed paste.")]
    disable_bracketed_paste: bool,

    #[clap(long = "log-enable")]
    enabled_log_events: Vec<TraceEvent>,

    #[clap(help = "Path to script to execute")]
    script_path: Option<String>,

    #[clap(help = "Arguments for script")]
    script_args: Vec<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, clap::ValueEnum)]
enum TraceEvent {
    #[clap(name = "arithmetic")]
    Arithmetic,
    #[clap(name = "complete")]
    Complete,
    #[clap(name = "expand")]
    Expand,
    #[clap(name = "parse")]
    Parse,
    #[clap(name = "pattern")]
    Pattern,
    #[clap(name = "tokenize")]
    Tokenize,
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
    //
    // Parse args.
    //
    let args: Vec<_> = std::env::args().collect();
    let parsed_args = CommandLineArgs::parse_from(&args);

    //
    // Initializing tracing.
    //
    let mut filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing_subscriber::filter::LevelFilter::INFO);

    let enabled_trace_events: HashSet<TraceEvent> =
        parsed_args.enabled_log_events.iter().cloned().collect();
    for event in enabled_trace_events {
        let targets = match event {
            TraceEvent::Arithmetic => vec!["parser::arithmetic"],
            TraceEvent::Complete => vec!["shell::completion", "shell::builtins::complete"],
            TraceEvent::Expand => vec![],
            TraceEvent::Parse => vec!["parse"],
            TraceEvent::Pattern => vec!["shell::pattern"],
            TraceEvent::Tokenize => vec!["tokenize"],
        };

        filter = filter.with_targets(
            targets
                .into_iter()
                .map(|target| (target, tracing::Level::DEBUG)),
        );
    }

    let stderr_log_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .without_time()
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(stderr_log_layer)
        .try_init()
        .expect("Failed to initialize tracing.");

    //
    // Run.
    //
    let result = tokio::runtime::Builder::new_multi_thread()
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

async fn run(
    cli_args: Vec<String>,
    args: CommandLineArgs,
) -> Result<u8, interactive_shell::InteractiveShellError> {
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

    let options = interactive_shell::Options {
        shell: shell::CreateOptions {
            login: args.login || argv0.as_ref().map_or(false, |a0| a0.starts_with('-')),
            interactive: args.is_interactive(),
            no_editing: args.no_editing,
            no_profile: args.no_profile,
            no_rc: args.no_rc,
            posix: args.posix || args.sh_mode,
            print_commands_and_arguments: args.print_commands_and_arguments,
            read_commands_from_stdin,
            shell_name: argv0,
            sh_mode: args.sh_mode,
            verbose: args.verbose,
        },
        disable_bracketed_paste: args.disable_bracketed_paste,
    };

    let mut shell = interactive_shell::InteractiveShell::new(&options).await?;

    if let Some(command) = args.command {
        // TODO: Use script_path as $0 and remaining args as positional parameters.
        let params = shell.shell().default_exec_params();
        shell.shell_mut().run_string(&command, &params).await?;
    } else if args.read_commands_from_stdin {
        shell.run_interactively().await?;
    } else if let Some(script_path) = args.script_path {
        shell
            .shell_mut()
            .run_script(Path::new(&script_path), args.script_args.as_slice())
            .await?;
    } else {
        shell.run_interactively().await?;
    }

    Ok(shell.shell().last_result())
}
