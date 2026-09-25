//! Generates artifacts derived from the `brush` command-line interface: man
//! pages, markdown help, shell completion scripts, and the config file's JSON
//! schema.
//!
//! Man pages and markdown are rendered from the command line's usage spec.
//! Completion scripts call back into `brush` itself for their answers, so they
//! need no other tool installed.
//!
//! This lives here, rather than in `xtask`, so that the build tooling doesn't
//! need to depend on the shell it builds. It's driven by `cargo xtask gen`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use usage::Cli;
use usage_lib::docs::manpage::ManpageRenderer;
use usage_lib::docs::markdown::MarkdownRenderer;

/// Generate artifacts derived from the brush command-line interface.
#[derive(Cli)]
#[usage(bin = "gen")]
struct GenCli {
    #[usage(subcommand)]
    command: GenCommand,
}

#[derive(usage::Subcommands)]
enum GenCommand {
    /// Generate man content into the given directory.
    Man(OutputDir),
    /// Generate help content in markdown format.
    Markdown(OutputPath),
    /// Generate a completion script, written to standard output.
    Completion(ShellName),
    /// Generate the JSON schema for the configuration file.
    Schema(OutputPath),
}

#[derive(usage::Args)]
struct OutputDir {
    /// Output directory.
    output_dir: PathBuf,
}

#[derive(usage::Args)]
struct OutputPath {
    /// Output file path.
    output_path: PathBuf,
}

#[derive(usage::Args)]
struct ShellName {
    /// Shell to generate a completion script for: bash, elvish, fish, nu,
    /// powershell, or zsh.
    shell: String,
}

fn main() -> Result<()> {
    let GenCli { command } = GenCli::parse();

    match command {
        GenCommand::Man(OutputDir { output_dir }) => {
            // The man renderer skips hidden subcommands but not hidden flags, such
            // as the `+o` and `+O` forms, so they're dropped here.
            let mut spec = spec()?;
            spec.cmd.flags.retain(|flag| !flag.hide);
            let manpage = ManpageRenderer::new(spec).render()?;
            std::fs::create_dir_all(&output_dir)?;
            std::fs::write(output_dir.join("brush.1"), manpage)?;
        }
        GenCommand::Markdown(OutputPath { output_path }) => {
            let markdown = MarkdownRenderer::new(spec()?).render_spec()?;
            std::fs::write(output_path, format!("{}\n", markdown.trim()))?;
        }
        GenCommand::Completion(ShellName { shell }) => {
            let shell = usage::complete::Shell::from_name(&shell)
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

/// Returns the shell command line's usage spec, as the documentation renderers
/// read it.
fn spec() -> Result<usage_lib::Spec> {
    Ok(brush_shell::args::CommandLineArgs::to_kdl().parse()?)
}
