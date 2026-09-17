//! CI workflow commands that aggregate multiple checks and tests.
//!
//! This module provides composite workflows that run multiple checks in sequence:
//!
//! ## Quick workflow (`cargo xtask ci quick`)
//!
//! Fast inner-loop checks (~7s warm cache) for rapid iteration, using nothing
//! but the Rust toolchain:
//! 1. **Format check** - Fast, catches formatting issues early
//! 2. **Build check** - Ensures code compiles with all features
//! 3. **Lint check** - Clippy warnings that should be addressed
//! 4. **Unit tests** - Fast tests excluding integration test binaries
//!
//! ## Full workflow (`cargo xtask ci full`)
//!
//! Comprehensive validation (~60s warm cache) before opening a pull request:
//! 1. All quick workflow checks
//! 2. **Pre-commit hooks** - File hygiene, spelling, workflow analysis, links,
//!    and the cargo-deny audit: everything in `.pre-commit-config.yaml`
//! 3. **Schema check** - Verifies generated schemas are up-to-date
//! 4. **Integration tests** - Full workspace tests including compat tests
//!
//! The ordering is intentional: fast checks run first to provide quick feedback,
//! with slower comprehensive tests running last.
//!
//! `ci full` reproduces every CI check that does not need a different machine or
//! toolchain. What it deliberately does not cover, because CI alone can: the
//! three-OS matrix, the MSRV toolchain, cross-compilation, coverage, benchmarks,
//! the bash-completion suite, the nightly-only `check unused-deps` and
//! `analyze public-api`, `CodeQL`, and SARIF upload.

use anyhow::Result;
use clap::Parser;

use crate::check::{self, BuildArgs, CheckCommand, HooksArgs};
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
    /// Use this for rapid iteration during development. Needs no tools beyond
    /// the Rust toolchain, which is why it is also what the optional pre-push
    /// git hook runs.
    Quick(QuickArgs),

    /// Run the full workflow: quick + pre-commit hooks, schemas, integration tests (~60s warm).
    ///
    /// This runs every check that should pass before opening a pull request.
    /// Requires `prek` in addition to the Rust toolchain; pass `--no-hooks`
    /// to skip the prek-backed portion, which includes the cargo-deny audit.
    Full(FullArgs),
}

/// Arguments for quick workflow.
#[derive(Parser)]
pub struct QuickArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Arguments for the full workflow.
#[derive(Parser)]
pub struct FullArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,

    /// Skip the pre-commit hooks (including the cargo-deny audit), which
    /// require prek to be installed.
    #[clap(long)]
    no_hooks: bool,
}

/// Run a CI workflow command.
pub fn run(cmd: &CiCommand, verbose: bool) -> Result<()> {
    match cmd {
        CiCommand::Quick(args) => run_quick(args, verbose),
        CiCommand::Full(args) => run_full(args, verbose),
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

/// The checks shared by every workflow, fastest first.
fn quick_steps(verbose: bool) -> Vec<Step<'static>> {
    vec![
        (
            "Format check",
            Box::new(move || check::run(&CheckCommand::Fmt, verbose)),
        ),
        (
            "Build check",
            Box::new(move || check::run(&CheckCommand::Build(BuildArgs::default()), verbose)),
        ),
        (
            "Lint check",
            Box::new(move || check::run(&CheckCommand::Lint, verbose)),
        ),
        (
            "Unit tests",
            Box::new(move || test::run(&make_unit_test_command(), verbose)),
        ),
    ]
}

/// Run quick inner-loop checks (~7s warm cache).
fn run_quick(args: &QuickArgs, verbose: bool) -> Result<()> {
    eprintln!("Running quick checks...\n");
    run_steps(
        &quick_steps(verbose),
        args.continue_on_error,
        "Quick checks",
    )
}

/// Run the full pre-PR workflow (~60s warm cache).
fn run_full(args: &FullArgs, verbose: bool) -> Result<()> {
    eprintln!("Running full checks...\n");

    let mut steps = quick_steps(verbose);

    if args.no_hooks {
        eprintln!("Skipping pre-commit hooks (--no-hooks).\n");
    } else {
        steps.push((
            "Pre-commit hooks",
            Box::new(move || check::run(&CheckCommand::Hooks(HooksArgs::default()), verbose)),
        ));
    }

    steps.extend([
        (
            "Schema check",
            Box::new(move || check::run(&CheckCommand::Schemas, verbose))
                as Box<dyn Fn() -> Result<()>>,
        ),
        (
            "Integration tests",
            Box::new(move || test::run(&make_integration_test_command(), verbose)),
        ),
    ]);

    run_steps(&steps, args.continue_on_error, "Full checks")
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
            if !continue_on_error {
                // `main` prints the full cause chain of a propagated error.
                return Err(e);
            }
            // Nothing downstream ever sees this error, so print its cause
            // chain here; that is where install hints and tool output live.
            for cause in e.chain().skip(1) {
                eprintln!("   caused by: {cause}");
            }
            failures.push(name);
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
