//! CI workflow commands that aggregate multiple checks and tests.

use anyhow::Result;
use clap::Parser;
use xshell::Shell;

use crate::check::{self, CheckCommand};
use crate::test::{self, BinaryArgs, TestCommand, TestSubcommand};

/// Run CI workflows.
#[derive(Parser)]
pub enum CiCommand {
    /// Run pre-commit workflow: build, deps, fmt, lint, schemas checks + all tests.
    ///
    /// This runs the essential checks that should pass before every commit.
    /// Does not include: bench, links, public-api, spelling, unused-deps, workflows.
    PreCommit(PreCommitArgs),
}

/// Arguments for pre-commit workflow.
#[derive(Parser)]
pub struct PreCommitArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Run a CI workflow command.
pub fn run(cmd: &CiCommand) -> Result<()> {
    match cmd {
        CiCommand::PreCommit(args) => run_pre_commit(args),
    }
}

/// Type alias for a named step in a CI workflow.
type Step = (&'static str, fn() -> Result<()>);

/// Create a `TestCommand` with default `BinaryArgs` and the given subcommand.
const fn make_test_command(subcommand: TestSubcommand) -> TestCommand {
    TestCommand {
        binary_args: BinaryArgs {
            brush_path: None,
            profile: crate::common::BuildProfile::Release,
            debug: false,
            release: false,
        },
        subcommand,
    }
}

fn run_pre_commit(args: &PreCommitArgs) -> Result<()> {
    let _sh = Shell::new()?;

    eprintln!("Running pre-commit checks...\n");

    let steps: &[Step] = &[
        ("Format check", || check::run(&CheckCommand::Fmt)),
        ("Lint check", || check::run(&CheckCommand::Lint)),
        ("Dependency check", || check::run(&CheckCommand::Deps)),
        ("Build check", || check::run(&CheckCommand::Build)),
        ("Schema check", || check::run(&CheckCommand::Schemas)),
        ("All tests", || {
            test::run(&make_test_command(TestSubcommand::All))
        }),
    ];

    let mut failures: Vec<&str> = Vec::new();

    for (name, step) in steps {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Running: {name}");
        eprintln!("{}\n", "=".repeat(60));

        if let Err(e) = step() {
            eprintln!("\n❌ {name} failed: {e}");
            if args.continue_on_error {
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
        eprintln!("Pre-commit checks completed with failures:");
        for name in &failures {
            eprintln!("  ❌ {name}");
        }
        eprintln!("{}", "=".repeat(60));
        anyhow::bail!("{} check(s) failed", failures.len());
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("✅ All pre-commit checks passed!");
    eprintln!("{}", "=".repeat(60));

    Ok(())
}
