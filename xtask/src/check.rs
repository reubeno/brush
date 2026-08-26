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
use std::collections::{BTreeMap, BTreeSet};
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
pub fn run(cmd: &CheckCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh, verbose),
        CheckCommand::Lint => check_lint(&sh, verbose),
        CheckCommand::Deps => check_deps(&sh, verbose),
        CheckCommand::UnusedDeps => check_unused_deps(&sh, verbose),
        CheckCommand::Build => check_build(&sh, verbose),
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
    let invocations = feature_sweep_invocations(&["clippy", "--all-targets"], sh, verbose)?;
    for invocation in &invocations {
        let mut invocation = invocation.clone();
        if verbose {
            // Ask cargo to be loud as well.
            invocation.push("--verbose".into());
            eprintln!("Running: cargo {}", invocation.join(" "));
        }
        run_cargo_invocation(sh, invocation)?;
    }
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
    let invocations =
        feature_sweep_invocations(&["+nightly", "udeps", "--all-targets"], sh, verbose)?;
    run_cargo_invocations(sh, invocations)?;
    eprintln!("Unused dependency check passed.");
    Ok(())
}

fn check_build(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking that code compiles...");
    let invocations = feature_sweep_invocations(&["check", "--all-targets"], sh, verbose)?;
    for invocation in &invocations {
        let mut invocation = invocation.clone();
        if verbose {
            // Ask cargo to be loud as well.
            invocation.push("--verbose".into());
            eprintln!("Running: cargo {}", invocation.join(" "));
        }
        run_cargo_invocation(sh, invocation)?;
    }
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

/// Crates whose manifests declare argument-parsing engine features
/// ([`ENGINE_FEATURES`]).
const ENGINE_CRATES: [&str; 2] = ["brush-core", "brush-builtins"];

/// Argument-parsing engine features. These are mutually exclusive by design
/// (see the guards in `brush-builtins`), so a naive `--all-features` sweep
/// cannot build the workspace; sweeps instead cover everything else wholesale
/// and exercise the engine-carrying crates once per engine.
const ENGINE_FEATURES: [&str; 3] = ["parser-bpaf", "parser-clap", "parser-usage"];

/// Extracts per-package `[features]` names from `cargo metadata` output.
fn parse_workspace_features(metadata_json: &str) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let root: serde_json::Value =
        serde_json::from_str(metadata_json).context("failed to parse `cargo metadata` output")?;

    let packages = root["packages"]
        .as_array()
        .context("`cargo metadata` output missing `packages` array")?;

    let mut workspace_features = BTreeMap::new();
    for package in packages {
        let name = package["name"]
            .as_str()
            .context("package entry missing `name`")?
            .to_owned();
        let features = package["features"]
            .as_object()
            .with_context(|| format!("package `{name}` missing `features` table"))?;

        workspace_features.insert(
            name,
            features.keys().map(|feature| feature.to_owned()).collect(),
        );
    }

    Ok(workspace_features)
}

/// Builds fully-qualified feature selections enabling every feature of
/// `crates`, except any engine other than `selected_engine`. Fully-qualified
/// (`crate/feature`) names are used so that unselected crates keep their own
/// defaults untouched.
fn qualified_features(
    workspace_features: &BTreeMap<String, BTreeSet<String>>,
    crates: &[&str],
    selected_engine: &str,
) -> Vec<String> {
    let mut features = Vec::new();
    for crate_name in crates {
        for feature in workspace_features.get(*crate_name).into_iter().flatten() {
            let is_other_engine =
                ENGINE_FEATURES.contains(&feature.as_str()) && feature != selected_engine;
            if !is_other_engine {
                features.push(format!("{crate_name}/{feature}"));
            }
        }
    }
    features
}

/// Builds cargo token lists (everything after `cargo`) covering every feature
/// combination reachable under the mutual-exclusion constraint on parser
/// engines:
///
/// 1. one wholesale sweep of every crate except the engine-carrying ones with `--all-features`,
///    then
/// 2. one sweep per engine over just those crates, with explicit feature selections.
fn feature_sweep_invocations(
    tool_tokens: &[&str],
    sh: &Shell,
    verbose: bool,
) -> Result<Vec<Vec<String>>> {
    if verbose {
        eprintln!("Resolving workspace features via cargo metadata...");
    }
    let metadata_json = cmd!(sh, "cargo metadata --format-version 1 --no-deps")
        .read()
        .context("failed to read `cargo metadata` output")?;
    let workspace_features = parse_workspace_features(&metadata_json)?;

    let mut invocations = Vec::new();

    // Sweep 1: everything except the engine-carrying crates.
    let mut broad: Vec<String> = tool_tokens.iter().map(|token| token.to_string()).collect();
    broad.push("--workspace".into());
    for crate_name in ENGINE_CRATES {
        broad.extend(["--exclude".to_owned(), crate_name.to_owned()]);
    }
    broad.push("--all-features".into());
    invocations.push(broad);

    // Sweeps 2+: the engine-carrying crates, once per engine.
    for engine in ENGINE_FEATURES {
        let mut targeted: Vec<String> = tool_tokens.iter().map(|token| token.to_string()).collect();
        targeted.push("--no-default-features".into());
        for crate_name in ENGINE_CRATES {
            targeted.extend(["-p".to_owned(), crate_name.to_owned()]);
        }
        targeted.extend(qualified_features(
            &workspace_features,
            &ENGINE_CRATES,
            engine,
        ));
        invocations.push(targeted);
    }

    Ok(invocations)
}

/// Runs each pre-built cargo invocation (tokens after `cargo`), failing fast
/// with the failing command echoed back.
fn run_cargo_invocations(sh: &Shell, invocations: Vec<Vec<String>>) -> Result<()> {
    for invocation in &invocations {
        run_cargo_invocation(sh, invocation.clone())?;
    }
    Ok(())
}

fn run_cargo_invocation(sh: &Shell, tokens: Vec<String>) -> Result<()> {
    let context = format!("`cargo {}` failed", tokens.join(" "));
    cmd!(sh, "cargo {tokens...}")
        .run()
        .with_context(|| context.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> &'static str {
        r#"{
            "packages": [
                {
                    "name": "brush-core",
                    "features": {
                        "default": [],
                        "serde": [],
                        "experimental-parser": [],
                        "parser-clap": [],
                        "parser-bpaf": [],
                        "parser-usage": []
                    }
                },
                {
                    "name": "brush-builtins",
                    "features": {
                        "parser-clap": [],
                        "parser-bpaf": [],
                        "parser-usage": [],
                        "builtin.alias": []
                    }
                },
                {
                    "name": "unrelated",
                    "features": { "shiny": [] }
                }
            ]
        }"#
    }

    #[test]
    fn parses_workspace_features() {
        let features = parse_workspace_features(sample_metadata()).unwrap();

        assert_eq!(features.len(), 3);
        assert!(features["brush-core"].contains("experimental-parser"));
        assert!(features["brush-builtins"].contains("builtin.alias"));
        assert!(features["unrelated"].contains("shiny"));
    }

    #[test]
    fn parse_workspace_features_rejects_malformed_input() {
        assert!(parse_workspace_features("not json").is_err());
    }

    #[test]
    fn qualified_features_select_single_engine() {
        let features = parse_workspace_features(sample_metadata()).unwrap();
        let selection = qualified_features(&features, &ENGINE_CRATES, "parser-bpaf")
            .into_iter()
            .collect::<BTreeSet<_>>();

        let expected = [
            "brush-core/default",
            "brush-core/experimental-parser",
            "brush-core/parser-bpaf",
            "brush-core/serde",
            "brush-builtins/builtin.alias",
            "brush-builtins/parser-bpaf",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

        assert_eq!(selection, expected);
    }

    #[test]
    fn sweeps_cover_non_engine_crates_and_each_engine() {
        let sh = Shell::new().unwrap();
        let invocations =
            feature_sweep_invocations(&["check", "--all-targets"], &sh, false).unwrap();

        // One broad sweep + one per engine.
        assert_eq!(invocations.len(), 1 + ENGINE_FEATURES.len());

        let broad = &invocations[0];
        assert!(broad.contains(&"--workspace".to_owned()));
        assert!(broad.contains(&"--all-features".to_owned()));
        assert_eq!(
            broad.iter().filter(|token| *token == "--exclude").count(),
            ENGINE_CRATES.len()
        );

        for invocation in &invocations[1..] {
            assert!(invocation.contains(&"--no-default-features".to_owned()));
            assert_eq!(
                invocation.iter().filter(|token| *token == "-p").count(),
                ENGINE_CRATES.len()
            );

            // Exactly one engine may appear in each targeted sweep, named once
            // per engine-carrying crate as `<crate>/<engine>`.
            let engines_present = ENGINE_FEATURES
                .iter()
                .filter(|engine| {
                    invocation
                        .iter()
                        .any(|token| token.contains('/') && token.ends_with(**engine))
                })
                .count();
            assert_eq!(engines_present, 1);
        }
    }
}
