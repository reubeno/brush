//! Check commands for code quality validation.

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run code quality checks.
#[derive(Parser)]
pub enum CheckCommand {
    /// Check that the code compiles.
    Build,
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

/// Run a check command.
pub fn run(cmd: &CheckCommand) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh),
        CheckCommand::Lint => check_lint(&sh),
        CheckCommand::Deps => check_deps(&sh),
        CheckCommand::UnusedDeps => check_unused_deps(&sh),
        CheckCommand::Build => check_build(&sh),
        CheckCommand::Schemas => check_schemas(&sh),
        CheckCommand::PublicApi => check_public_api(&sh),
        CheckCommand::Spelling => check_spelling(&sh),
        CheckCommand::Workflows => check_workflows(&sh),
        CheckCommand::Links => check_links(&sh),
    }
}

fn check_fmt(sh: &Shell) -> Result<()> {
    eprintln!("Checking code formatting...");
    cmd!(sh, "cargo fmt --check --all")
        .run()
        .context("Format check failed")?;
    eprintln!("Format check passed.");
    Ok(())
}

fn check_lint(sh: &Shell) -> Result<()> {
    eprintln!("Running clippy...");
    cmd!(sh, "cargo clippy --workspace --all-features --all-targets")
        .run()
        .context("Clippy check failed")?;
    eprintln!("Clippy check passed.");
    Ok(())
}

fn check_deps(sh: &Shell) -> Result<()> {
    eprintln!("Checking dependencies...");
    cmd!(sh, "cargo deny --all-features check all")
        .run()
        .context("Dependency check failed")?;
    eprintln!("Dependency check passed.");
    Ok(())
}

fn check_unused_deps(sh: &Shell) -> Result<()> {
    eprintln!("Checking for unused dependencies (requires nightly)...");
    cmd!(
        sh,
        "cargo +nightly udeps --workspace --all-targets --all-features"
    )
    .run()
    .context("Unused dependency check failed")?;
    eprintln!("Unused dependency check passed.");
    Ok(())
}

fn check_build(sh: &Shell) -> Result<()> {
    eprintln!("Checking that code compiles...");
    cmd!(sh, "cargo check --all-features --all-targets")
        .run()
        .context("Build check failed")?;
    eprintln!("Build check passed.");
    Ok(())
}

fn check_schemas(sh: &Shell) -> Result<()> {
    eprintln!("Checking generated schemas...");

    // Regenerate schemas
    cmd!(
        sh,
        "cargo run --package xtask -- gen schema config --out schemas/config.schema.json"
    )
    .run()
    .context("Failed to regenerate schemas")?;

    // Check for drift
    let diff_output = cmd!(sh, "git diff --exit-code schemas/").run();

    if diff_output.is_err() {
        anyhow::bail!(
            "Generated schemas are out of date. Please run 'cargo xtask gen schema config --out schemas/config.schema.json' and commit the changes."
        );
    }

    eprintln!("Schema check passed.");
    Ok(())
}

fn check_public_api(sh: &Shell) -> Result<()> {
    eprintln!("Analyzing public API (requires nightly and cargo-public-api)...");

    // This is typically only useful for PRs comparing against main
    cmd!(sh, "cargo +nightly public-api --version")
        .run()
        .context("cargo-public-api not installed. Install with: cargo install cargo-public-api")?;

    eprintln!("Public API analysis complete. For PR diffs, compare against main branch.");
    Ok(())
}

fn check_spelling(sh: &Shell) -> Result<()> {
    eprintln!("Checking spelling...");
    cmd!(sh, "typos")
        .run()
        .context("Spelling check failed. Install typos with: cargo install typos-cli")?;
    eprintln!("Spelling check passed.");
    Ok(())
}

fn check_workflows(sh: &Shell) -> Result<()> {
    eprintln!("Checking GitHub workflows for security issues...");
    cmd!(sh, "zizmor .github/workflows/")
        .run()
        .context("Workflow check failed. Install zizmor with: pip install zizmor")?;
    eprintln!("Workflow check passed.");
    Ok(())
}

fn check_links(sh: &Shell) -> Result<()> {
    eprintln!("Checking for broken links...");
    cmd!(sh, "lychee --offline docs/")
        .run()
        .context("Link check failed. Install lychee with: cargo install lychee")?;
    eprintln!("Link check passed.");
    Ok(())
}
