//! Check commands for code quality validation.
//!
//! This module provides various code quality checks that can be run individually
//! or as part of a CI workflow. Each check wraps an external tool and provides
//! consistent error handling and verbose output.
//!
//! Some checks require additional tools to be installed:
//! - `cargo-deny`: Security/license auditing (`cargo install cargo-deny`)
//! - `cargo-udeps`: Unused dependency detection (`cargo install cargo-udeps`, requires nightly)
//! - `cargo-public-api`: Public API analysis (`cargo install cargo-public-api`, requires nightly)
//! - `typos`: Spelling checker (`cargo install typos-cli`)
//! - `zizmor`: GitHub workflow security scanner (`pip install zizmor`)
//! - `lychee`: Link checker (`cargo install lychee`)

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run code quality checks.
#[derive(Parser)]
pub enum CheckCommand {
    /// Check that the code compiles.
    Build(BuildArgs),
    /// Check dependencies for security vulnerabilities and license compliance.
    Deps,
    /// Check code formatting.
    Fmt,
    /// Check for broken links in documentation.
    Links,
    /// Run clippy lints.
    Lint,
    /// Analyze public API for breaking changes (requires nightly).
    PublicApi,
    /// Check that generated schemas are up-to-date.
    Schemas,
    /// Check for spelling errors.
    Spelling,
    /// Check for unused dependencies (requires nightly).
    UnusedDeps,
    /// Check GitHub workflow files for security issues.
    Workflows,
}

/// Options for the build check.
#[derive(Default, Parser)]
pub struct BuildArgs {
    /// Only check crates that build with the workspace-wide MSRV; for use when
    /// the check is being run with the oldest toolchain the workspace as a
    /// whole supports.
    #[clap(long = "workspace-msrv")]
    workspace_msrv: bool,
}

/// Run a check command.
pub fn run(cmd: &CheckCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh, verbose),
        CheckCommand::Lint => check_lint(&sh, verbose),
        CheckCommand::Deps => check_deps(&sh, verbose),
        CheckCommand::UnusedDeps => check_unused_deps(&sh, verbose),
        CheckCommand::Build(args) => check_build(&sh, args, verbose),
        CheckCommand::Schemas => check_schemas(&sh, verbose),
        CheckCommand::PublicApi => check_public_api(&sh, verbose),
        CheckCommand::Spelling => check_spelling(&sh, verbose),
        CheckCommand::Workflows => check_workflows(&sh, verbose),
        CheckCommand::Links => check_links(&sh, verbose),
    }
}

fn check_fmt(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking code formatting...");
    if verbose {
        eprintln!("Running: cargo fmt --check --all");
    }
    cmd!(sh, "cargo fmt --check --all")
        .run()
        .context("Format check failed")?;
    eprintln!("Format check passed.");
    Ok(())
}

fn check_lint(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Running clippy...");
    let mut args = vec!["clippy", "--workspace", "--all-features", "--all-targets"];
    if verbose {
        args.push("--verbose");
        eprintln!("Running: cargo {}", args.join(" "));
    }
    cmd!(sh, "cargo {args...}")
        .run()
        .context("Clippy check failed")?;
    eprintln!("Clippy check passed.");
    Ok(())
}

fn check_deps(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking dependencies...");
    if verbose {
        eprintln!("Running: cargo deny --all-features check all");
    }
    cmd!(sh, "cargo deny --all-features check all")
        .run()
        .context("Dependency check failed")?;
    eprintln!("Dependency check passed.");
    Ok(())
}

fn check_unused_deps(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking for unused dependencies (requires nightly)...");
    if verbose {
        eprintln!("Running: cargo +nightly udeps --workspace --all-targets --all-features");
    }
    cmd!(
        sh,
        "cargo +nightly udeps --workspace --all-targets --all-features"
    )
    .run()
    .context("Unused dependency check failed")?;
    eprintln!("Unused dependency check passed.");
    Ok(())
}

/// Turns a `rust-version` value into something orderable.
fn msrv_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Finds the workspace crates that declare a `rust-version` higher than the
/// lowest one in the workspace, and so can't be built with the oldest toolchain
/// the workspace as a whole supports. Derived from cargo metadata so that the
/// manifests remain the only place this is recorded.
fn crates_above_workspace_msrv(sh: &Shell) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
    }

    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        rust_version: Option<String>,
    }

    // `--no-deps` narrows the output to workspace members.
    let json = cmd!(sh, "cargo metadata --no-deps --format-version 1")
        .quiet()
        .read()
        .context("Failed to read cargo metadata")?;
    let metadata: Metadata =
        serde_json::from_str(&json).context("Failed to parse cargo metadata")?;

    let Some(workspace_msrv) = metadata
        .packages
        .iter()
        .filter_map(|p| p.rust_version.as_deref())
        .map(msrv_key)
        .min()
    else {
        return Ok(Vec::new());
    };

    Ok(metadata
        .packages
        .iter()
        .filter(|p| {
            p.rust_version
                .as_deref()
                .is_some_and(|v| msrv_key(v) > workspace_msrv)
        })
        .map(|p| p.name.clone())
        .collect())
}

fn check_build(sh: &Shell, args: &BuildArgs, verbose: bool) -> Result<()> {
    eprintln!("Checking that code compiles...");

    let excluded = if args.workspace_msrv {
        crates_above_workspace_msrv(sh)?
    } else {
        Vec::new()
    };
    if !excluded.is_empty() {
        eprintln!(
            "Skipping crates with a higher MSRV than the workspace: {}",
            excluded.join(", ")
        );
    }

    let mut args = vec!["check", "--all-features", "--all-targets", "--workspace"];
    for name in &excluded {
        args.push("--exclude");
        args.push(name);
    }
    if verbose {
        args.push("--verbose");
        eprintln!("Running: cargo {}", args.join(" "));
    }
    cmd!(sh, "cargo {args...}")
        .run()
        .context("Build check failed")?;
    eprintln!("Build check passed.");
    Ok(())
}

fn check_schemas(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking generated schemas...");

    // Regenerate schemas to a temporary state to compare against committed versions.
    if verbose {
        eprintln!(
            "Running: cargo run --package xtask -- gen schema config --out schemas/config.schema.json"
        );
    }
    cmd!(
        sh,
        "cargo run --package xtask -- gen schema config --out schemas/config.schema.json"
    )
    .run()
    .context("Failed to regenerate schemas")?;

    // Check for drift by capturing the diff output.
    // We don't use --exit-code here because we want to capture and display the
    // actual differences to help the user understand what changed.
    if verbose {
        eprintln!("Running: git diff schemas/");
    }
    let diff_output = cmd!(sh, "git diff schemas/")
        .read()
        .context("Failed to run git diff on schemas directory")?;

    if !diff_output.is_empty() {
        // Show the user exactly what changed so they can understand the drift.
        eprintln!("\nSchema drift detected. The following changes were found:\n");
        eprintln!("{diff_output}");
        anyhow::bail!(
            "Generated schemas are out of date. Please run 'cargo xtask gen schema config --out schemas/config.schema.json' and commit the changes."
        );
    }

    eprintln!("Schema check passed.");
    Ok(())
}

fn check_public_api(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Analyzing public API (requires nightly and cargo-public-api)...");

    // This is typically only useful for PRs comparing against main
    if verbose {
        eprintln!("Running: cargo +nightly public-api --version");
    }
    cmd!(sh, "cargo +nightly public-api --version")
        .run()
        .context("cargo-public-api not installed. Install with: cargo install cargo-public-api")?;

    eprintln!("Public API analysis complete. For PR diffs, compare against main branch.");
    Ok(())
}

fn check_spelling(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking spelling...");
    if verbose {
        eprintln!("Running: typos");
    }
    cmd!(sh, "typos")
        .run()
        .context("Spelling check failed. Install typos with: cargo install typos-cli")?;
    eprintln!("Spelling check passed.");
    Ok(())
}

fn check_workflows(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking GitHub workflows for security issues...");
    if verbose {
        eprintln!("Running: zizmor .github/workflows/");
    }
    cmd!(sh, "zizmor .github/workflows/")
        .run()
        .context("Workflow check failed. Install zizmor with: pip install zizmor")?;
    eprintln!("Workflow check passed.");
    Ok(())
}

fn check_links(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking for broken links...");
    if verbose {
        eprintln!("Running: lychee --offline docs/");
    }
    cmd!(sh, "lychee --offline docs/")
        .run()
        .context("Link check failed. Install lychee with: cargo install lychee")?;
    eprintln!("Link check passed.");
    Ok(())
}
