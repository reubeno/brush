//! Generation commands for documentation, completions, and schemas.
//!
//! This module provides commands for generating various artifacts:
//! - **Documentation**: Man pages and markdown help text from the shell's
//!   command-line parser (bpaf)
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
#[derive(Clone, Copy, Debug, Parser)]
pub enum CompletionCommand {
    /// Generate completion script for `bash`.
    Bash,
    /// Generate completion script for `elvish`.
    Elvish,
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
        GenCommand::Completion(completion_cmd) => gen_completion_script(*completion_cmd, verbose),
        GenCommand::Schema(schema_cmd) => match schema_cmd {
            SchemaCommand::Config(args) => gen_config_schema(args, verbose),
        },
    }
}

/// Renders the shell's help content using its bpaf-based parser.
fn render_help_text() -> Result<String> {
    let parser = brush_shell::args::CommandLineArgs::option_parser();
    match parser.run_inner(&["--help"][..]) {
        Err(failure) => Ok(failure.unwrap_stdout()),
        Ok(_) => anyhow::bail!("unexpectedly parsed --help"),
    }
}

fn gen_man(args: &GenerateManArgs, verbose: bool) -> Result<()> {
    use std::fmt::Write as _;

    if verbose {
        eprintln!("Generating man pages to: {}", args.output_dir.display());
    }

    // Create the output dir if it doesn't exist. If it already does, we proceed
    // onward and hope for the best.
    if !args.output_dir.exists() {
        std::fs::create_dir_all(&args.output_dir)?;
    }

    // Generate a simple roff-formatted man page from the rendered help text.
    let help = render_help_text()?;
    let mut man = String::new();
    writeln!(&mut man, ".TH BRUSH 1 \"brush\" \"\" \"User Commands\"")?;
    writeln!(&mut man, ".SH NAME")?;
    writeln!(&mut man, "brush \\- Bo[u]rn[e] RUsty SHell")?;
    writeln!(&mut man, ".SH SYNOPSIS")?;
    writeln!(&mut man, ".nf")?;
    writeln!(&mut man, "{}", help.lines().next().unwrap_or_default())?;
    writeln!(&mut man, ".fi")?;
    writeln!(&mut man, ".SH OPTIONS")?;
    writeln!(&mut man, ".nf")?;
    for line in help.lines().skip(1) {
        writeln!(&mut man, "{line}")?;
    }
    writeln!(&mut man, ".fi")?;

    let output_path = args.output_dir.join("brush.1");
    std::fs::write(output_path, man)?;

    Ok(())
}

fn gen_markdown_docs(args: &GenerateMarkdownArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating markdown docs to: {}",
            args.output_path.display()
        );
    }

    // Generate markdown from the bpaf-rendered help text.
    let help = render_help_text()?;
    let markdown = format!("# brush\n\n```text\n{help}\n```\n");

    if let Some(parent) = args.output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output_path, markdown)?;

    Ok(())
}

/// Generate a shell completion script to stdout.
///
/// N.B. Completions are generated at runtime by the shell binary itself (via
/// bpaf), so this shells out to a locally built binary.
fn gen_completion_script(shell: CompletionCommand, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Generating {shell:?} completion script...");
    }

    let style_flag = match shell {
        CompletionCommand::Bash => "--bpaf-complete-style-bash",
        CompletionCommand::Elvish => "--bpaf-complete-style-elvish",
        CompletionCommand::Fish => "--bpaf-complete-style-fish",
        CompletionCommand::PowerShell => {
            anyhow::bail!("PowerShell completions are not supported by bpaf")
        }
        CompletionCommand::Zsh => "--bpaf-complete-style-zsh",
    };

    // Locate or build the brush binary.
    let sh = Shell::new()?;
    let workspace_root = std::env::current_dir()?;
    let binary = workspace_root.join("target/debug/brush");
    if !binary.exists() {
        cmd!(sh, "cargo build -p brush-shell").run()?;
    }

    let script = cmd!(sh, "{binary} {style_flag}")
        .read()
        .context("Failed to generate completion script")?;
    print!("{script}");

    Ok(())
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
