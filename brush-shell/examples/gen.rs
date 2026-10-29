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
use clap::Parser;
use winnow_args::Args as _;
use winnow_args::complete::Shell;

/// Generate artifacts derived from the brush command-line interface.
#[derive(Parser)]
enum GenCommand {
    /// Generate a completion script, written to standard output.
    Completion {
        /// Shell to generate a completion script for: bash, elvish, fish, powershell, or zsh.
        shell: String,
    },
    /// Generate the JSON schema for the configuration file.
    Schema {
        /// Output file path.
        output_path: PathBuf,
    },
}

fn main() -> Result<()> {
    match GenCommand::parse() {
        GenCommand::Completion { shell } => {
            let shell = Shell::from_name(&shell)
                .with_context(|| format!("no completion script for shell `{shell}`"))?;
            print!(
                "{}",
                brush_shell::args::CommandLineArgs::completion_script(shell)
            );
        }
        GenCommand::Schema { output_path } => {
            let schema = schemars::schema_for!(brush_shell::config::Config);
            let json = serde_json::to_string_pretty(&schema)?;
            std::fs::write(&output_path, format!("{json}\n"))?;
        }
    }

    Ok(())
}
