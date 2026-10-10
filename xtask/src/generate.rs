//! Generation commands for documentation, completions, and schemas.
//!
//! This module provides commands for generating various artifacts:
//! - **Documentation**: Man pages and markdown help text, for the command line and for
//!   the builtins
//! - **Completions**: Shell completion scripts for bash, zsh, fish, etc., which call
//!   back into `brush` for their answers
//! - **Schemas**: JSON schemas for configuration files
//! - **Distribution archives**: Reproducible documentation bundles with checksums
//!
//! Man pages and markdown come from the fragments winnow-args' derives write
//! while `brush-shell` is checked with `WINNOW_ARGS_SPEC` set: one TOML file
//! per command-line type. Nothing is linked or run for them, so `--target`
//! describes the command line of any installed target.
//!
//! Completion scripts and the config schema are produced by the `gen` example
//! in `brush-shell`, which this module shells out to. Either way xtask stays
//! free of any dependency on the shell it's used to build.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use winnow_args_man::Manual;
use winnow_args_spec::Catalog;
use xshell::{Shell, cmd};

use crate::common::find_workspace_root;

/// The crate, as the derives name its directory, and the type of the `brush`
/// command line.
const SHELL_CRATE: &str = "brush_shell";
const SHELL_COMMAND: &str = "CommandLineArgs";
/// The crate that holds the builtins.
const BUILTINS_CRATE: &str = "brush_builtins";

/// Run the `gen` example in `brush-shell` with the given arguments.
///
/// The example is built with brush-shell's default features plus `schema`; the
/// artifacts it generates describe the command-line interface as it appears
/// with that feature set.
fn run_gen_example(sh: &Shell, args: &[&OsStr]) -> Result<()> {
    cmd!(
        sh,
        "cargo run --package brush-shell --features schema --example gen -- {args...}"
    )
    .run()
    .context("Failed to run the gen example in brush-shell")?;
    Ok(())
}

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
    /// Generate a man page and a markdown page for each builtin.
    Builtins(GenerateBuiltinsArgs),
    /// Generate a reproducible documentation distribution archive with checksums.
    Dist(GenerateDistArgs),
}

/// Completion script generation commands.
#[derive(Parser)]
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

    /// Describe the command line as this target triple compiles it.
    #[clap(long)]
    target: Option<String>,
}

/// Arguments for markdown documentation generation.
#[derive(Parser)]
pub struct GenerateMarkdownArgs {
    /// Output file path.
    #[clap(long = "out", short = 'o')]
    output_path: PathBuf,

    /// Describe the command line as this target triple compiles it.
    #[clap(long)]
    target: Option<String>,
}

/// Arguments for builtin documentation generation.
#[derive(Parser)]
pub struct GenerateBuiltinsArgs {
    /// Output directory; pages go to its `man` and `md` subdirectories.
    #[clap(long = "output-dir", short = 'o')]
    output_dir: PathBuf,

    /// Describe the builtins as this target triple compiles them.
    #[clap(long)]
    target: Option<String>,
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
    let sh = Shell::new()?;
    match cmd {
        GenCommand::Docs(docs_cmd) => match docs_cmd {
            DocsCommand::Man(args) => gen_man(&sh, args, verbose),
            DocsCommand::Markdown(args) => gen_markdown_docs(&sh, args, verbose),
            DocsCommand::Builtins(args) => gen_builtin_docs(&sh, args, verbose),
            DocsCommand::Dist(args) => gen_docs_dist(&sh, args, verbose),
        },
        GenCommand::Completion(completion_cmd) => {
            // These names are the ones understood by winnow-args' `complete::Shell`.
            let shell = match completion_cmd {
                CompletionCommand::Bash => "bash",
                CompletionCommand::Elvish => "elvish",
                CompletionCommand::Fish => "fish",
                CompletionCommand::PowerShell => "powershell",
                CompletionCommand::Zsh => "zsh",
            };
            gen_completion_script(&sh, shell, verbose)
        }
        GenCommand::Schema(schema_cmd) => match schema_cmd {
            SchemaCommand::Config(args) => gen_config_schema(&sh, args, verbose),
        },
    }
}

fn gen_man(sh: &Shell, args: &GenerateManArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Generating man pages to: {}", args.output_dir.display());
    }

    let spec = write_fragments(sh, args.target.as_deref())?;
    let command = Catalog::load(&spec.join(SHELL_CRATE))?.stitch(SHELL_COMMAND)?;
    let pages = Manual::default().render_pages(&command, &command.name)?;
    write_pages(&args.output_dir, &pages)
}

fn gen_markdown_docs(sh: &Shell, args: &GenerateMarkdownArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating markdown docs to: {}",
            args.output_path.display()
        );
    }

    let spec = write_fragments(sh, args.target.as_deref())?;
    let command = Catalog::load(&spec.join(SHELL_CRATE))?.stitch(SHELL_COMMAND)?;
    // `brush` has no subcommands, so its one page is the whole of it.
    let (_, page) = winnow_args_markdown::render_pages(&command, &command.name)?
        .into_iter()
        .next()
        .context("the command line has no page")?;
    std::fs::write(&args.output_path, page)
        .with_context(|| format!("Failed to write {}", args.output_path.display()))
}

fn gen_builtin_docs(sh: &Shell, args: &GenerateBuiltinsArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Generating builtin docs to: {}", args.output_dir.display());
    }

    let spec = write_fragments(sh, args.target.as_deref())?;
    let (mut man, mut markdown) = (Vec::new(), Vec::new());
    // Two crates may each have a type of one name, so each is loaded alone.
    for crate_name in [BUILTINS_CRATE, SHELL_CRATE] {
        let catalog = Catalog::load(&spec.join(crate_name))?;
        for ty in catalog.roots() {
            let Some(name) = builtin_name(&ty) else {
                continue;
            };
            let command = catalog.stitch(&ty)?;
            let name = if command.name.is_empty() {
                name
            } else {
                command.name.clone()
            };
            man.extend(Manual::default().render_pages(&command, &name)?);
            markdown.extend(winnow_args_markdown::render_pages(&command, &name)?);
        }
    }
    write_pages(&args.output_dir.join("man"), &man)?;
    write_pages(&args.output_dir.join("md"), &markdown)
}

/// The word a builtin's pages go by, from the name of its type; `None` for a
/// type that is not a builtin of its own.
///
/// A fragment knows a type's name, not the names it is registered under in
/// `brush-builtins`, so an alias (`typeset`, `readonly`, `[`) has no page.
fn builtin_name(ty: &str) -> Option<String> {
    match ty {
        SHELL_COMMAND | "UnimplementedCommand" => None,
        "DotCommand" => Some("source".to_owned()),
        _ => ty.strip_suffix("Command").map(str::to_lowercase),
    }
}

/// Check `brush-shell` with `WINNOW_ARGS_SPEC` set, and return the directory
/// the derives wrote to: one subdirectory per crate.
///
/// Cargo compiles a crate that derives again when that variable changes. A
/// new directory each time has every such crate write its fragments, and the
/// check keeps a target directory of its own so that ordinary builds are not
/// the ones recompiled.
fn write_fragments(sh: &Shell, target: Option<&str>) -> Result<PathBuf> {
    let work = find_workspace_root()?.join("target").join("xtask-docs");
    let runs = work.join("spec");
    if runs.exists() {
        std::fs::remove_dir_all(&runs)
            .with_context(|| format!("Failed to remove {}", runs.display()))?;
    }
    let run = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let spec = runs.join(run.to_string());
    let target_dir = work.join("target");
    let target_args = target.map(|triple| ["--target", triple]);
    let target_args = target_args.as_ref().map_or(&[][..], |args| &args[..]);
    cmd!(
        sh,
        "cargo check --package brush-shell --target-dir {target_dir} {target_args...}"
    )
    .env("WINNOW_ARGS_SPEC", &spec)
    .run()
    .context("Failed to check brush-shell for its command-line fragments")?;
    Ok(spec)
}

/// Write each `(file name, contents)` pair into `dir`.
fn write_pages(dir: &Path, pages: &[(String, String)]) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    for (name, contents) in pages {
        let path = dir.join(name);
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Generate a shell completion script to stdout.
///
/// The completion script is written directly to stdout so it can be piped
/// to a file or sourced directly by the shell.
fn gen_completion_script(sh: &Shell, shell: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Generating {shell} completion script...");
    }
    run_gen_example(sh, &[OsStr::new("completion"), OsStr::new(shell)])
}

fn gen_config_schema(sh: &Shell, args: &GenerateSchemaArgs, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Generating config schema to: {}",
            args.output_path.display()
        );
    }

    run_gen_example(sh, &[OsStr::new("schema"), args.output_path.as_os_str()])
}

fn gen_docs_dist(sh: &Shell, args: &GenerateDistArgs, verbose: bool) -> Result<()> {
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
        target: None,
    };
    gen_markdown_docs(sh, &md_args, verbose)?;

    // Generate man pages
    let man_args = GenerateManArgs {
        output_dir: man_dir,
        target: None,
    };
    gen_man(sh, &man_args, verbose)?;

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
