//! Generates artifacts derived from the `brush` command-line interface: man
//! pages, markdown help, shell completion scripts, and the config file's JSON
//! schema.
//!
//! This lives here, rather than in `xtask`, so that the build tooling doesn't
//! need to depend on the shell it builds. It's driven by `cargo xtask gen`.

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser};

/// Generate artifacts derived from the brush command-line interface.
#[derive(Parser)]
enum GenCommand {
    /// Generate man content into the given directory.
    Man {
        /// Output directory.
        output_dir: PathBuf,
    },
    /// Generate help content in markdown format.
    Markdown {
        /// Output file path.
        output_path: PathBuf,
    },
    /// Generate a completion script, written to standard output.
    Completion {
        /// Shell to generate a completion script for.
        shell: clap_complete::Shell,
    },
    /// Generate the JSON schema for the configuration file.
    Schema {
        /// Output file path.
        output_path: PathBuf,
    },
}

fn main() -> Result<()> {
    match GenCommand::parse() {
        GenCommand::Man { output_dir } => {
            std::fs::create_dir_all(&output_dir)?;
            clap_mangen::generate_to(brush_shell::args::CommandLineArgs::command(), &output_dir)?;
        }
        GenCommand::Markdown { output_path } => {
            let options = clap_markdown::MarkdownOptions::new()
                .show_footer(false)
                .show_table_of_contents(true);
            let markdown =
                clap_markdown::help_markdown_custom::<brush_shell::args::CommandLineArgs>(&options);
            std::fs::write(&output_path, markdown)?;
        }
        GenCommand::Completion { shell } => {
            let mut cmd = brush_shell::args::CommandLineArgs::command();
            clap_complete::generate(shell, &mut cmd, "brush", &mut std::io::stdout());
        }
        GenCommand::Schema { output_path } => {
            let schema = schemars::schema_for!(brush_shell::config::Config);
            let json = serde_json::to_string_pretty(&schema)?;
            std::fs::write(&output_path, format!("{json}\n"))?;
        }
    }

    Ok(())
}
