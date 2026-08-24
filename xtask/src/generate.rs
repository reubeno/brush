//! Generation commands for documentation, completions, and schemas.
//!
//! This module provides commands for generating various artifacts:
//! - **Documentation**: Man pages and markdown help text from clap definitions
//! - **Completions**: Shell completion scripts for bash, zsh, fish, etc.
//! - **Schemas**: JSON schemas for configuration files
//! - **Distribution archives**: Reproducible documentation bundles with checksums

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Generate various artifacts.
#[derive(Parser)]
pub enum GenCommand {
    /// Generate completion scripts.
    #[clap(subcommand)]
    Completion(CompletionCommand),
    /// Generate documentation.
    #[clap(subcommand)]
    Docs(DocsCommand),
    /// Generate JSON schemas.
    #[clap(subcommand)]
    Schema(SchemaCommand),
}

/// Documentation generation commands.
#[derive(Parser)]
pub enum DocsCommand {
    /// Generate man content.
    Man(GenerateManArgs),
    /// Generate help content in markdown format.
    Markdown(GenerateMarkdownArgs),
    /// Generate a reproducible documentation distribution archive with checksums.
    Dist(GenerateDistArgs),
}

/// Completion script generation commands.
#[derive(Parser)]
pub enum CompletionCommand {
    /// Generate completion script for `bash`.
    Bash,
    /// Generate completion script for `fish`.
    Fish,
    /// Generate completion script for `PowerShell`.
    PowerShell,
    /// Generate completion script for `zsh`.
    Zsh,
}

/// Arguments for man page generation.
#[derive(Parser)]
pub struct GenerateManArgs {
    /// Output directory.
    #[clap(long = "output-dir", short = 'o')]
    output_dir: PathBuf,
}

/// Arguments for markdown documentation generation.
#[derive(Parser)]
pub struct GenerateMarkdownArgs {
    /// Output file path.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

/// Arguments for documentation distribution generation.
#[derive(Parser)]
pub struct GenerateDistArgs {
    /// Output file path for the distribution archive (defaults to brush-docs.tar.gz).
    #[clap(long = "out", short = 'o', default_value = "brush-docs.tar.gz")]
    output_path: PathBuf,

    /// Generate SHA-256 checksum file alongside the distribution archive.
    #[clap(long, default_value_t = true)]
    sha256: bool,

    /// Generate SHA-512 checksum file alongside the distribution archive.
    #[clap(long, default_value_t = true)]
    sha512: bool,
}

/// Schema generation commands.
#[derive(Parser)]
pub enum SchemaCommand {
    /// Generate JSON schema for the configuration file.
    Config(GenerateSchemaArgs),
}

/// Arguments for schema generation.
#[derive(Parser)]
pub struct GenerateSchemaArgs {
    /// Output file path.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,
}

/// Run a generation command.
pub fn run(cmd: &GenCommand, verbose: bool) -> Result<()> {
    match cmd {
        GenCommand::Docs(docs_cmd) => match docs_cmd {
            DocsCommand::Man(args) => gen_man(args, verbose),
            DocsCommand::Markdown(args) => gen_markdown_docs(args, verbose),
            DocsCommand::Dist(args) => gen_docs_dist(args, verbose),
        },
        GenCommand::Completion(completion_cmd) => {
            let shell = match completion_cmd {
                CompletionCommand::Bash => usage::complete::Shell::Bash,
                CompletionCommand::Fish => usage::complete::Shell::Fish,
                CompletionCommand::PowerShell => usage::complete::Shell::PowerShell,
                CompletionCommand::Zsh => usage::complete::Shell::Zsh,
            };
            gen_completion_script(shell, verbose);
            Ok(())
        }
        GenCommand::Schema(schema_cmd) => match schema_cmd {
            SchemaCommand::Config(args) => gen_config_schema(args, verbose),
        },
    }
}

fn gen_man(args: &GenerateManArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating spec for man pages to: {}",
            args.output_dir.display()
        );
    }

    // TODO(usage-migration): man pages are now rendered by the `usage` tool from
    // the emitted KDL spec (`usage g manpage -f brush.usage.kdl`); here we only
    // emit the spec.
    if !args.output_dir.exists() {
        std::fs::create_dir_all(&args.output_dir)?;
    }
    write_usage_spec(&args.output_dir.join("brush.usage.kdl"))?;

    Ok(())
}

fn gen_markdown_docs(args: &GenerateMarkdownArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating markdown docs to: {}",
            args.output_path.display()
        );
    }

    // TODO(usage-migration): markdown docs are now rendered by the `usage` tool
    // (`usage g markdown -f <spec> --out-dir <dir>`); here we only emit the spec.
    write_usage_spec(&args.output_path)?;

    Ok(())
}

/// Emits the brush CLI's usage spec (KDL) to the given path.
fn write_usage_spec(path: &std::path::Path) -> Result<()> {
    let kdl = brush_shell::args::CommandLineArgs::to_kdl();
    std::fs::write(path, kdl)?;
    Ok(())
}

/// Generate a shell completion script to stdout.
///
/// The completion script is written directly to stdout so it can be piped
/// to a file or sourced directly by the shell.
fn gen_completion_script(shell: usage::complete::Shell, verbose: bool) {
    if verbose {
        eprintln!("Generating {} completion script...", shell.as_str());
    }
    print!(
        "{}",
        brush_shell::args::CommandLineArgs::completion_script(shell)
    );
}

fn gen_config_schema(args: &GenerateSchemaArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating config schema to: {}",
            args.output_path.display()
        );
    }

    // Generate JSON schema for the configuration file.
    let schema = schemars::schema_for!(brush_shell::config::Config);
    let json = serde_json::to_string_pretty(&schema)?;
    std::fs::write(&args.output_path, format!("{json}\n"))?;

    Ok(())
}

fn gen_docs_dist(args: &GenerateDistArgs, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    // Create a temporary directory for staging the documentation
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let staging_dir = temp_dir.path();
    let md_dir = staging_dir.join("md");
    let man_dir = staging_dir.join("man");

    std::fs::create_dir_all(&md_dir)?;
    std::fs::create_dir_all(&man_dir)?;

    if verbose {
        eprintln!("Staging documentation in: {}", staging_dir.display());
    }

    // Generate markdown documentation
    let md_args = GenerateMarkdownArgs {
        output_path: md_dir.join("brush.md"),
    };
    gen_markdown_docs(&md_args, verbose)?;

    // Generate man pages
    let man_args = GenerateManArgs {
        output_dir: man_dir,
    };
    gen_man(&man_args, verbose)?;

    // Get absolute path for output
    let output_path = if args.output_path.is_absolute() {
        args.output_path.clone()
    } else {
        std::env::current_dir()?.join(&args.output_path)
    };

    if verbose {
        eprintln!(
            "Creating reproducible distribution archive: {}",
            output_path.display()
        );
    }

    // Create reproducible distribution archive using tar with options for reproducibility:
    // - --sort=name: Sort files by name for consistent ordering
    // - --mtime: Set modification time to epoch for reproducibility
    // - --owner=0 --group=0: Remove user/group ownership info
    // - --numeric-owner: Use numeric IDs
    // - --pax-option: Remove atime/ctime from PAX headers
    let output_path_str = output_path.display().to_string();

    // Change to staging directory and create archive
    let dir_guard = sh.push_dir(staging_dir);

    cmd!(
        sh,
        "tar --sort=name --mtime=1970-01-01T00:00:00Z --owner=0 --group=0 --numeric-owner --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime -czf {output_path_str} ."
    )
    .run()
    .context("Failed to create distribution archive")?;

    eprintln!("Created: {}", output_path.display());

    // Generate checksums
    drop(dir_guard);

    if args.sha256 {
        let checksum_path = format!("{}.sha256", output_path.display());
        let checksum = cmd!(sh, "sha256sum {output_path_str}")
            .read()
            .context("Failed to generate SHA-256 checksum")?;
        std::fs::write(&checksum_path, format!("{checksum}\n"))?;
        if verbose {
            eprintln!("Created: {checksum_path}");
        }
    }

    if args.sha512 {
        let checksum_path = format!("{}.sha512", output_path.display());
        let checksum = cmd!(sh, "sha512sum {output_path_str}")
            .read()
            .context("Failed to generate SHA-512 checksum")?;
        std::fs::write(&checksum_path, format!("{checksum}\n"))?;
        if verbose {
            eprintln!("Created: {checksum_path}");
        }
    }

    Ok(())
}
