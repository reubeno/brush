//! Check commands for code quality validation.
//!
//! This module provides various code quality checks that can be run individually
//! or as part of a CI workflow. Each check wraps an external tool and provides
//! consistent error handling and verbose output.
//!
//! Some checks require additional tools to be installed:
//! - `cargo-udeps`: Unused dependency detection (`cargo install cargo-udeps`, requires nightly)
//! - `prek`: Runs everything defined in `.pre-commit-config.yaml` -- typos,
//!   zizmor, lychee, cargo-deny, and the file hygiene hooks (`cargo binstall prek`)
//!
//! Where a check is defined: checks that need project knowledge (which crates
//! sit above the workspace MSRV, how schemas are regenerated) or a component of
//! the pinned toolchain (rustfmt, clippy) live here. Third-party tools whose
//! versions nothing else pins -- including cargo subcommands such as
//! cargo-deny, which are ordinary crates rather than toolchain components --
//! are declared in `.pre-commit-config.yaml`, where Dependabot keeps them
//! current and CI's hooks workflow runs the same `cargo xtask check hooks` a
//! contributor runs. `cargo-udeps` is the exception: its upstream hook is
//! `language: system`, so a hook entry would pin nothing, and it needs nightly.
//!
//! Note that hooks may rewrite files, exactly as they do when run as git hooks;
//! `check hooks` is a fixer, not a read-only check.

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run code quality checks.
#[derive(Parser)]
pub enum CheckCommand {
    /// Check that the code compiles.
    Build(BuildArgs),
    /// Check code formatting.
    Fmt,
    /// Run the hooks defined in `.pre-commit-config.yaml`: file hygiene,
    /// spelling, workflow analysis, links, dependency audit. May rewrite files.
    Hooks(HooksArgs),
    /// Run clippy lints.
    Lint,
    /// Check that generated schemas are up-to-date.
    Schemas,
    /// Check for unused dependencies (requires nightly).
    UnusedDeps,
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

/// Options for the hooks check.
#[derive(Default, Parser)]
pub struct HooksArgs {
    /// Hooks to run, by id or by alias as listed in `.pre-commit-config.yaml`
    /// (e.g. `spelling`, `links`, `workflows`, `deps`). Runs every hook when
    /// omitted.
    hooks: Vec<String>,
}

/// Run a check command.
pub fn run(cmd: &CheckCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh, verbose),
        CheckCommand::Lint => check_lint(&sh, verbose),
        CheckCommand::UnusedDeps => check_unused_deps(&sh, verbose),
        CheckCommand::Build(args) => check_build(&sh, args, verbose),
        CheckCommand::Schemas => check_schemas(&sh, verbose),
        CheckCommand::Hooks(args) => check_hooks(&sh, args, verbose),
    }
}

/// Fails with install instructions when prek is missing, so that an absent tool
/// reads as an absent tool rather than as a failing check.
fn ensure_prek(sh: &Shell) -> Result<()> {
    cmd!(sh, "prek --version")
        .quiet()
        .ignore_stdout()
        .ignore_stderr()
        .run()
        .context(
            "prek was not found on PATH. It runs the linters pinned in \
             .pre-commit-config.yaml. Install it with one of:\n  \
             cargo binstall prek\n  uv tool install prek\n  brew install prek",
        )
}

/// Runs hooks from `.pre-commit-config.yaml` over the whole tree. An empty
/// `hooks` slice runs all of them.
fn run_hooks(sh: &Shell, hooks: &[String], verbose: bool) -> Result<()> {
    // prek and pre-commit set this in every hook's environment. The pre-push
    // hook runs `cargo xtask ci quick`; if that ever becomes `ci full`, or a
    // hook otherwise reaches back here, fail instead of recursing forever.
    if std::env::var_os("PRE_COMMIT").is_some() {
        anyhow::bail!("refusing to run the pre-commit hooks from inside a pre-commit hook");
    }

    ensure_prek(sh)?;

    let mut args = vec!["run", "--all-files"];
    args.extend(hooks.iter().map(String::as_str));

    if verbose {
        eprintln!("Running: prek {}", args.join(" "));
    }

    cmd!(sh, "prek {args...}").run()?;
    Ok(())
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

fn check_hooks(sh: &Shell, args: &HooksArgs, verbose: bool) -> Result<()> {
    eprintln!("Running pre-commit hooks...");
    run_hooks(sh, &args.hooks, verbose).context("Pre-commit hooks failed")?;
    eprintln!("Pre-commit hooks passed.");
    Ok(())
}
