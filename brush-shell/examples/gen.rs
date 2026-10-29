//! Generates the artifacts that need `brush-shell` linked: shell completion
//! scripts and the config file's JSON schema.
//!
//! Completion scripts call back into `brush` itself for their answers, so they
//! need no other tool installed. Man pages and markdown need nothing linked;
//! `cargo xtask gen docs` writes those.
//!
//! This lives here, rather than in `xtask`, so that the build tooling doesn't
//! need to depend on the shell it builds. It's driven by `cargo xtask gen`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use winnow_args::Args;
use winnow_args::complete::Shell;

/// Generate artifacts derived from the brush command-line interface.
#[derive(Args)]
#[arg(name = "gen")]
struct GenCli {
    #[arg(subcommand)]
    command: GenCommand,
}

#[derive(winnow_args::Subcommand)]
enum GenCommand {
    /// Generate a completion script, written to standard output.
    Completion(ShellName),
    /// Generate the JSON schema for the configuration file.
    Schema(OutputPath),
}

#[derive(Args)]
struct OutputPath {
    /// Output file path.
    #[arg(positional)]
    output_path: PathBuf,
}

#[derive(Args)]
struct ShellName {
    /// Shell to generate a completion script for: bash, fish, or zsh.
    #[arg(positional)]
    shell: String,
}

fn main() -> Result<()> {
    let GenCli { command } = GenCli::parse();

    match command {
        GenCommand::Completion(ShellName { shell }) => {
            let shell = Shell::from_name(&shell)
                .with_context(|| format!("no completion script for shell `{shell}`"))?;
            print!(
                "{}",
                brush_shell::args::CommandLineArgs::completion_script(shell)
            );
        }
        GenCommand::Schema(OutputPath { output_path }) => {
            let schema = schemars::schema_for!(brush_shell::config::Config);
            let json = serde_json::to_string_pretty(&schema)?;
            std::fs::write(output_path, format!("{json}\n"))?;
        }
    }

    Ok(())
}
