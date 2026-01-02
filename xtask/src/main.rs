//! xtask-style command-line tool for building this project.

mod analyze;
mod check;
mod ci;
mod common;
mod generate;
mod test;

use anyhow::Result;
use clap::Parser;

/// Global options shared across all commands.
#[derive(Parser, Debug, Clone, Copy)]
pub struct GlobalArgs {
    /// Enable verbose output.
    #[clap(long, short = 'v', global = true)]
    pub verbose: bool,
}

#[derive(Parser)]
#[clap(name = "xtask", about = "Build automation tasks for brush")]
struct CommandLineArgs {
    #[clap(flatten)]
    global: GlobalArgs,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Run analysis tasks (benchmarks, public API diffing).
    #[clap(subcommand)]
    Analyze(analyze::AnalyzeCommand),
    /// Run code quality checks.
    #[clap(subcommand)]
    Check(check::CheckCommand),
    /// Run CI workflows.
    #[clap(subcommand)]
    Ci(ci::CiCommand),
    /// Generate documentation, completions, and schemas.
    #[clap(subcommand)]
    Gen(generate::GenCommand),
    /// Run tests.
    Test(test::TestCommand),
}

fn main() -> Result<()> {
    let args = CommandLineArgs::parse();

    match &args.command {
        Command::Analyze(cmd) => analyze::run(cmd),
        Command::Gen(cmd) => generate::run(cmd),
        Command::Check(cmd) => check::run(cmd),
        Command::Test(cmd) => test::run(cmd),
        Command::Ci(cmd) => ci::run(cmd),
    }
}
