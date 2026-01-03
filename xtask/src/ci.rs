//! CI workflow commands that aggregate multiple checks and tests.
//!
//! This module provides composite workflows that run multiple checks in sequence:
//!
//! ## Quick workflow (`cargo xtask ci quick`)
//!
//! Fast inner-loop checks (~7s warm cache) for rapid iteration:
//! 1. **Format check** - Fast, catches formatting issues early
//! 2. **Build check** - Ensures code compiles with all features
//! 3. **Lint check** - Clippy warnings that should be addressed
//! 4. **Unit tests** - Fast tests excluding integration test binaries
//!
//! ## Pre-commit workflow (`cargo xtask ci pre-commit`)
//!
//! Comprehensive validation (~45s warm cache) before committing:
//! 1. All quick workflow checks
//! 2. **Dependency check** - Security vulnerabilities and license compliance
//! 3. **Schema check** - Verifies generated schemas are up-to-date
//! 4. **Integration tests** - Full workspace tests including compat tests
//!
//! The ordering is intentional: fast checks run first to provide quick feedback,
//! with slower comprehensive tests running last.

use anyhow::Result;
use clap::Parser;

use crate::check::{self, CheckCommand};
use crate::test::{
    self, BinaryArgs, IntegrationTestArgs, TestCommand, TestSubcommand, UnitTestArgs,
};

/// Type alias for a named step in a CI workflow.
type Step<'a> = (&'a str, Box<dyn Fn() -> Result<()> + 'a>);

/// Run CI workflows.
#[derive(Parser)]
pub enum CiCommand {
    /// Run quick inner-loop checks: fmt, build, lint, unit tests (~7s warm).
    ///
    /// Use this for rapid iteration during development.
    Quick(QuickArgs),

    /// Run full pre-commit workflow: quick + deps, schemas, integration tests (~45s warm).
    ///
    /// This runs all essential checks that should pass before every commit.
    /// Does not include: bench, links, public-api, spelling, unused-deps, workflows.
    PreCommit(PreCommitArgs),
}

/// Arguments for quick workflow.
#[derive(Parser)]
pub struct QuickArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Arguments for pre-commit workflow.
#[derive(Parser)]
pub struct PreCommitArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Run a CI workflow command.
pub fn run(cmd: &CiCommand, verbose: bool) -> Result<()> {
    match cmd {
        CiCommand::Quick(args) => run_quick(args, verbose),
        CiCommand::PreCommit(args) => run_pre_commit(args, verbose),
    }
}

/// Create a `TestCommand` for unit tests.
fn make_unit_test_command() -> TestCommand {
    TestCommand {
        binary_args: BinaryArgs {
            brush_path: None,
            profile: crate::common::BuildProfile::Debug,
            debug: false,
            release: false,
        },
        subcommand: TestSubcommand::Unit(UnitTestArgs::default()),
    }
}

/// Create a `TestCommand` for integration tests.
fn make_integration_test_command() -> TestCommand {
    TestCommand {
        binary_args: BinaryArgs {
            brush_path: None,
            profile: crate::common::BuildProfile::Debug,
            debug: false,
            release: false,
        },
        subcommand: TestSubcommand::Integration(IntegrationTestArgs::default()),
    }
}

/// Run quick inner-loop checks (~7s warm cache).
fn run_quick(args: &QuickArgs, verbose: bool) -> Result<()> {
    eprintln!("Running quick checks...\n");

    let steps: Vec<Step<'_>> = vec![
        (
            "Format check",
            Box::new(|| check::run(&CheckCommand::Fmt, verbose)),
        ),
        (
            "Build check",
            Box::new(|| check::run(&CheckCommand::Build, verbose)),
        ),
        (
            "Lint check",
            Box::new(|| check::run(&CheckCommand::Lint, verbose)),
        ),
        (
            "Unit tests",
            Box::new(|| test::run(&make_unit_test_command(), verbose)),
        ),
    ];

    run_steps(&steps, args.continue_on_error, "Quick checks")
}

fn run_pre_commit(args: &PreCommitArgs, verbose: bool) -> Result<()> {
    eprintln!("Running pre-commit checks...\n");

    let steps: Vec<Step<'_>> = vec![
        // Quick checks first
        (
            "Format check",
            Box::new(|| check::run(&CheckCommand::Fmt, verbose)),
        ),
        (
            "Build check",
            Box::new(|| check::run(&CheckCommand::Build, verbose)),
        ),
        (
            "Lint check",
            Box::new(|| check::run(&CheckCommand::Lint, verbose)),
        ),
        (
            "Unit tests",
            Box::new(|| test::run(&make_unit_test_command(), verbose)),
        ),
        // Additional pre-commit checks
        (
            "Dependency check",
            Box::new(|| check::run(&CheckCommand::Deps, verbose)),
        ),
        (
            "Schema check",
            Box::new(|| check::run(&CheckCommand::Schemas, verbose)),
        ),
        (
            "Integration tests",
            Box::new(|| test::run(&make_integration_test_command(), verbose)),
        ),
    ];

    run_steps(&steps, args.continue_on_error, "Pre-commit checks")
}

/// Run a series of steps, optionally continuing on error.
fn run_steps(steps: &[Step<'_>], continue_on_error: bool, workflow_name: &str) -> Result<()> {
    let mut failures: Vec<&str> = Vec::new();

    for (name, step) in steps {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Running: {name}");
        eprintln!("{}\n", "=".repeat(60));

        if let Err(e) = step() {
            eprintln!("\n❌ {name} failed: {e}");
            if continue_on_error {
                failures.push(name);
            } else {
                return Err(e);
            }
        } else {
            eprintln!("\n✅ {name} passed");
        }
    }

    if !failures.is_empty() {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("{workflow_name} completed with failures:");
        for name in &failures {
            eprintln!("  ❌ {name}");
        }
        eprintln!("{}", "=".repeat(60));
        anyhow::bail!("{} check(s) failed", failures.len());
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("✅ All {workflow_name} passed!");
    eprintln!("{}", "=".repeat(60));

    Ok(())
}
