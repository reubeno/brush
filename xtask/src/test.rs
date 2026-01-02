//! Test commands for running various test suites.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use xshell::{Shell, cmd};

use crate::common::{BuildProfile, find_brush_binary, find_workspace_root};

/// Shared arguments for test commands that need a brush binary.
#[derive(Args, Debug, Clone)]
pub struct BinaryArgs {
    /// Path to the brush binary to test. If not specified, uses the binary
    /// from the workspace's target directory based on --profile/--debug/--release.
    #[clap(long, global = true)]
    pub brush_path: Option<PathBuf>,

    /// Build profile to use when auto-detecting the brush binary.
    #[clap(long, short = 'p', value_enum, default_value_t = BuildProfile::Debug, global = true)]
    pub profile: BuildProfile,

    /// Use debug build profile (shorthand for --profile=debug).
    #[clap(long, conflicts_with_all = ["profile", "release"], global = true)]
    pub debug: bool,

    /// Use release build profile (shorthand for --profile=release).
    #[clap(long, conflicts_with_all = ["profile", "debug"], global = true)]
    pub release: bool,
}

impl BinaryArgs {
    /// Resolve the effective build profile, considering --debug/--release shorthands.
    #[must_use]
    pub const fn effective_profile(&self) -> BuildProfile {
        if self.debug {
            BuildProfile::Debug
        } else if self.release {
            BuildProfile::Release
        } else {
            self.profile
        }
    }

    /// Find the brush binary using these arguments.
    pub fn find_brush_binary(&self) -> Result<PathBuf> {
        find_brush_binary(self.brush_path.as_ref(), self.effective_profile())
    }
}

/// Run tests.
#[derive(Parser)]
pub struct TestCommand {
    /// Shared binary arguments.
    #[clap(flatten)]
    pub binary_args: BinaryArgs,

    /// Test subcommand.
    #[clap(subcommand)]
    pub subcommand: TestSubcommand,
}

/// Test subcommands.
#[derive(Subcommand)]
pub enum TestSubcommand {
    /// Run all tests (unit + compat).
    All,
    /// Run compatibility tests against bash.
    Compat,
    /// Run tests with code coverage collection.
    Coverage(CoverageArgs),
    /// Run external test suites.
    #[clap(subcommand)]
    External(ExternalTestCommand),
    /// Run unit tests with cargo-nextest.
    Unit,
}

/// Arguments for coverage collection.
#[derive(Args)]
pub struct CoverageArgs {
    /// Output file for coverage report (Cobertura XML format).
    #[clap(long, short = 'o', default_value = "codecov.xml")]
    output: PathBuf,
}

/// External test suite commands.
#[derive(Subcommand)]
pub enum ExternalTestCommand {
    /// Run the bash-completion test suite against brush.
    BashCompletion(BashCompletionArgs),
}

/// Arguments for bash-completion test suite.
#[derive(Args)]
pub struct BashCompletionArgs {
    /// Path to the bash-completion repository checkout.
    #[clap(long)]
    bash_completion_path: PathBuf,

    /// Output file for JSON test results.
    #[clap(long, short = 'o')]
    output: Option<PathBuf>,

    /// Output file for markdown summary report.
    #[clap(long)]
    summary_output: Option<PathBuf>,

    /// Path to the summarize-pytest-results.py script (for generating summary).
    /// Defaults to ./scripts/summarize-pytest-results.py relative to workspace root.
    #[clap(long)]
    summary_script: Option<PathBuf>,

    /// Number of parallel test workers.
    #[clap(long, short = 'j', default_value = "128")]
    jobs: u32,
}

/// Run a test command.
pub fn run(cmd: &TestCommand) -> Result<()> {
    let sh = Shell::new()?;

    match &cmd.subcommand {
        TestSubcommand::All => run_all_tests(&sh, &cmd.binary_args),
        TestSubcommand::Compat => run_compat_tests(&sh, &cmd.binary_args),
        TestSubcommand::Coverage(args) => run_coverage(&sh, &cmd.binary_args, args),
        TestSubcommand::External(ext_cmd) => run_external(ext_cmd, &cmd.binary_args, &sh),
        TestSubcommand::Unit => run_unit_tests(&sh, &cmd.binary_args),
    }
}

fn run_external(cmd: &ExternalTestCommand, binary_args: &BinaryArgs, sh: &Shell) -> Result<()> {
    match cmd {
        ExternalTestCommand::BashCompletion(args) => {
            run_bash_completion_tests(sh, args, binary_args)
        }
    }
}

fn run_unit_tests(sh: &Shell, binary_args: &BinaryArgs) -> Result<()> {
    let profile = binary_args.effective_profile();
    eprintln!("Running unit tests ({profile:?} profile)...");

    let mut args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];
    if profile == BuildProfile::Release {
        args.push("--release");
    }

    cmd!(sh, "cargo {args...}")
        .run()
        .context("Unit tests failed")?;
    eprintln!("Unit tests passed.");
    Ok(())
}

fn run_compat_tests(sh: &Shell, binary_args: &BinaryArgs) -> Result<()> {
    let profile = binary_args.effective_profile();
    eprintln!("Running compatibility tests ({profile:?} profile)...");

    let mut args = vec!["test", "--test", "brush-compat-tests"];
    if profile == BuildProfile::Release {
        args.push("--release");
    }

    cmd!(sh, "cargo {args...}")
        .run()
        .context("Compatibility tests failed")?;
    eprintln!("Compatibility tests passed.");
    Ok(())
}

fn run_all_tests(sh: &Shell, binary_args: &BinaryArgs) -> Result<()> {
    eprintln!("Running all tests...");

    // Run unit tests first
    run_unit_tests(sh, binary_args)?;

    // Then run compatibility tests
    run_compat_tests(sh, binary_args)?;

    eprintln!("All tests passed.");
    Ok(())
}

fn run_coverage(sh: &Shell, binary_args: &BinaryArgs, args: &CoverageArgs) -> Result<()> {
    let profile = binary_args.effective_profile();
    let output_path = args.output.display().to_string();

    eprintln!("Running tests with coverage ({profile:?} profile)...");
    eprintln!("Coverage output: {output_path}");

    // Set up llvm-cov environment
    eprintln!("Setting up llvm-cov environment...");
    let env_output = cmd!(sh, "cargo llvm-cov show-env --export-prefix")
        .read()
        .context("Failed to get llvm-cov environment. Is cargo-llvm-cov installed?")?;

    // Parse and set environment variables from llvm-cov output
    for line in env_output.lines() {
        if let Some(rest) = line.strip_prefix("export ") {
            if let Some((key, value)) = rest.split_once('=') {
                // Remove quotes if present
                let value = value.trim_matches('"').trim_matches('\'');
                sh.set_var(key, value);
            }
        }
    }

    // Clean previous coverage data
    cmd!(sh, "cargo llvm-cov clean --workspace")
        .run()
        .context("Failed to clean coverage data")?;

    // Build cargo nextest args
    let mut test_args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];
    if profile == BuildProfile::Release {
        test_args.push("--release");
    }

    // Run tests (allow failures, we still want to generate coverage report)
    let test_result = cmd!(sh, "cargo {test_args...}").ignore_status().run();

    if let Err(e) = &test_result {
        eprintln!("Warning: Test execution had issues: {e}");
    }

    // Generate coverage report
    eprintln!("Generating coverage report...");
    cmd!(
        sh,
        "cargo llvm-cov report --cobertura --output-path {output_path}"
    )
    .run()
    .context("Failed to generate coverage report")?;

    eprintln!("Coverage report written to: {output_path}");

    // Return the test result (so we fail if tests failed)
    test_result.context("Tests failed")?;
    eprintln!("Tests with coverage completed successfully.");
    Ok(())
}

fn run_bash_completion_tests(
    sh: &Shell,
    args: &BashCompletionArgs,
    binary_args: &BinaryArgs,
) -> Result<()> {
    eprintln!("Running bash-completion test suite...");

    // Find the brush binary (use explicit path or auto-detect from target dir)
    let brush_path = binary_args.find_brush_binary()?;
    eprintln!("Using brush binary: {}", brush_path.display());

    let test_dir = args.bash_completion_path.join("test");
    if !test_dir.exists() {
        anyhow::bail!(
            "bash-completion test directory not found at: {}",
            test_dir.display()
        );
    }

    let brush_path_str = brush_path.display().to_string();
    let jobs = args.jobs.to_string();

    // Build the pytest command
    let dir_guard = sh.push_dir(&test_dir);

    // Set environment variable for the test suite
    let _env = sh.push_env(
        "BASH_COMPLETION_TEST_BASH",
        format!("{brush_path_str} --noprofile --no-config --input-backend=basic"),
    );

    // Determine output arguments
    let json_output = args.output.as_ref().map_or_else(
        || "test-results-bash-completion.json".to_string(),
        |p| p.display().to_string(),
    );

    // Build the json report file argument
    let json_report_arg = format!("--json-report-file={json_output}");

    // Run pytest - we use ignore_status because some tests may fail
    // and that's expected (we're testing compatibility)
    let result = cmd!(sh, "pytest")
        .args(["-n", &jobs, "--json-report", &json_report_arg, "./t"])
        .ignore_status()
        .run();

    if let Err(e) = result {
        eprintln!("Warning: pytest execution had issues: {e}");
    }

    // Generate summary report if requested
    if let Some(summary_path) = &args.summary_output {
        let summary_path_str = summary_path.display().to_string();

        // Determine the script path - use provided path or default to
        // ./scripts/summarize-pytest-results.py relative to workspace root
        let script_path = args.summary_script.as_ref().map_or_else(
            || {
                find_workspace_root().map_or_else(
                    |_| PathBuf::from("./scripts/summarize-pytest-results.py"),
                    |root| root.join("scripts/summarize-pytest-results.py"),
                )
            },
            |p| p.clone(),
        );

        let script_path_str = script_path.display().to_string();

        // Go back to original directory for the script (if we were in test dir)
        drop(dir_guard);

        let title = "Test Summary: bash-completion test suite";
        let summary_result = cmd!(sh, "python3 {script_path_str}")
            .args(["-r", &json_output, "--title", title])
            .read();

        match summary_result {
            Ok(summary) => {
                sh.write_file(summary_path, &summary)?;
                eprintln!("Summary report written to: {summary_path_str}");
            }
            Err(e) => {
                eprintln!("Warning: Failed to generate summary report: {e}");
            }
        }
    }

    eprintln!("bash-completion test suite completed.");
    eprintln!("Results written to: {json_output}");
    Ok(())
}
