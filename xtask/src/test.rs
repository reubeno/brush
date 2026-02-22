//! Test commands for running various test suites.
//!
//! This module provides commands for running different types of tests:
//!
//! - **Unit tests**: Fast tests that don't execute the brush binary (excludes integration test
//!   binaries like brush-compat-tests, brush-interactive-tests, brush-completion-tests)
//! - **Integration tests**: All workspace tests including unit tests and integration tests that
//!   execute the brush binary
//! - **External suites**: Third-party test suites like bash-completion
//!
//! Both unit and integration tests support optional coverage collection via
//! `cargo-llvm-cov`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use xshell::{Shell, cmd};

use crate::common::{BuildProfile, find_brush_binary, find_workspace_root};

/// Integration test binaries that are excluded from unit tests.
/// These tests execute the brush binary and are slower.
const INTEGRATION_TEST_BINARIES: &[&str] = &[
    "brush-compat-tests",
    "brush-interactive-tests",
    "brush-completion-tests",
];

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
#[derive(Subcommand, Clone)]
pub enum TestSubcommand {
    /// Run unit tests (fast tests that don't execute the brush binary).
    ///
    /// Excludes integration test binaries: brush-compat-tests, brush-interactive-tests,
    /// brush-completion-tests.
    Unit(UnitTestArgs),

    /// Run all workspace tests (unit + integration tests).
    ///
    /// This includes all tests: unit tests plus integration tests that execute
    /// the brush binary (compat tests, interactive tests, completion tests).
    Integration(IntegrationTestArgs),

    /// Run external test suites.
    #[clap(subcommand)]
    External(ExternalTestCommand),
}

/// Arguments for unit tests.
#[derive(Args, Clone, Default)]
pub struct UnitTestArgs {
    /// Coverage options.
    #[clap(flatten)]
    pub coverage: CoverageArgs,
}

/// Arguments for integration tests.
#[derive(Args, Clone, Default)]
pub struct IntegrationTestArgs {
    /// Coverage options.
    #[clap(flatten)]
    pub coverage: CoverageArgs,

    /// Additional Cargo features to enable (e.g., "experimental-parser").
    /// Passed through as `--features <value>` to cargo nextest.
    #[clap(long)]
    pub features: Option<String>,

    /// Extra arguments to pass to the brush binary during test runs.
    /// Set as the `BRUSH_ARGS` environment variable before running nextest.
    #[clap(long)]
    pub brush_args: Option<String>,
}

/// Arguments for coverage collection.
#[derive(Args, Clone, Default)]
pub struct CoverageArgs {
    /// Collect code coverage during test run.
    #[clap(long)]
    pub coverage: bool,

    /// Output file for coverage report (Cobertura XML format).
    /// Only used when --coverage is specified.
    #[clap(long, short = 'o', default_value = "codecov.xml")]
    pub coverage_output: PathBuf,

    /// Skip cleaning previous coverage data.
    /// Use this when running multiple test suites to accumulate coverage.
    #[clap(long)]
    pub coverage_no_clean: bool,

    /// Run tests but do not generate coverage report.
    /// Use this to accumulate coverage across multiple test runs, then run
    /// with --coverage-report-only to generate the final merged report.
    #[clap(long)]
    pub coverage_no_report: bool,

    /// Only generate coverage report from previously accumulated data.
    /// Use after running tests with --coverage --coverage-no-report.
    #[clap(long)]
    pub coverage_report_only: bool,
}

/// External test suite commands.
#[derive(Subcommand, Clone)]
pub enum ExternalTestCommand {
    /// Run the bash-completion test suite against brush.
    BashCompletion(BashCompletionArgs),
}

/// Arguments for bash-completion test suite.
#[derive(Args, Clone)]
pub struct BashCompletionArgs {
    /// Path to the bash-completion repository checkout.
    #[clap(long)]
    bash_completion_path: PathBuf,

    /// List available tests without running them.
    #[clap(long)]
    list: bool,

    /// Filter tests by name pattern (passed to pytest -k).
    /// Supports pytest expression syntax, e.g., `"test_alias"`, `"test_alias and test_1"`.
    #[clap(long, short = 't')]
    test_filter: Option<String>,

    /// Run only specific test file(s). Can be specified multiple times.
    /// Example: `-f test_alias.py -f test_bash.py`
    #[clap(long, short = 'f')]
    file: Vec<String>,

    /// Stop on first test failure.
    #[clap(long, short = 'x')]
    stop_on_first: bool,

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

    /// Number of parallel test workers (requires pytest-xdist).
    /// Use -j 1 to disable parallel execution.
    #[clap(long, short = 'j', default_value = "128")]
    jobs: u32,
}

/// Run a test command.
pub fn run(cmd: &TestCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match &cmd.subcommand {
        TestSubcommand::Unit(args) => run_unit_tests(&sh, &cmd.binary_args, args, verbose),
        TestSubcommand::Integration(args) => {
            run_integration_tests(&sh, &cmd.binary_args, args, verbose)
        }
        TestSubcommand::External(ext_cmd) => run_external(ext_cmd, &cmd.binary_args, &sh, verbose),
    }
}

fn run_external(
    cmd: &ExternalTestCommand,
    binary_args: &BinaryArgs,
    sh: &Shell,
    verbose: bool,
) -> Result<()> {
    match cmd {
        ExternalTestCommand::BashCompletion(args) => {
            run_bash_completion_tests(sh, args, binary_args, verbose)
        }
    }
}

/// Run unit tests (excludes integration test binaries).
///
/// Unit tests are fast tests that don't execute the brush binary.
pub fn run_unit_tests(
    sh: &Shell,
    binary_args: &BinaryArgs,
    args: &UnitTestArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();
    eprintln!("Running unit tests ({profile:?} profile)...");

    // Build the filter expression to exclude integration test binaries
    let exclusions: Vec<String> = INTEGRATION_TEST_BINARIES
        .iter()
        .map(|name| format!("not binary({name})"))
        .collect();
    let filter_expr = exclusions.join(" and ");

    if args.coverage.coverage {
        run_tests_with_coverage(
            sh,
            profile,
            Some(&filter_expr),
            &args.coverage,
            None,
            None,
            verbose,
        )
    } else {
        run_nextest(sh, profile, Some(&filter_expr), None, None, verbose)?;
        eprintln!("Unit tests passed.");
        Ok(())
    }
}

/// Run all workspace tests (unit + integration).
///
/// This runs all tests in the workspace, including integration tests
/// that execute the brush binary.
pub fn run_integration_tests(
    sh: &Shell,
    binary_args: &BinaryArgs,
    args: &IntegrationTestArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();
    eprintln!("Running integration tests ({profile:?} profile)...");

    if args.coverage.coverage {
        run_tests_with_coverage(
            sh,
            profile,
            None,
            &args.coverage,
            args.features.as_deref(),
            args.brush_args.as_deref(),
            verbose,
        )
    } else {
        run_nextest(
            sh,
            profile,
            None,
            args.features.as_deref(),
            args.brush_args.as_deref(),
            verbose,
        )?;
        eprintln!("Integration tests passed.");
        Ok(())
    }
}

/// Run cargo nextest with optional filter expression.
fn run_nextest(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
    features: Option<&str>,
    brush_args: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let mut args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];

    if profile == BuildProfile::Release {
        args.push("--release");
    }

    // Add filter expression if provided
    let filter_value = filter_expr.map(str::to_string);
    if let Some(ref value) = filter_value {
        args.push("-E");
        args.push(value);
    }

    // Add features if provided
    let features_value = features.map(str::to_string);
    if let Some(ref value) = features_value {
        args.push("--features");
        args.push(value);
    }

    if verbose {
        eprintln!("Running: cargo {}", args.join(" "));
    }

    // Set BRUSH_ARGS env var if provided
    let _env_guard = brush_args.map(|val| sh.push_env("BRUSH_ARGS", val));

    cmd!(sh, "cargo {args...}").run().context("Tests failed")?;
    Ok(())
}

/// Run tests with code coverage collection using `cargo-llvm-cov`.
///
/// The coverage workflow:
/// 1. Source environment variables from `cargo llvm-cov show-env`
/// 2. Clean previous coverage data (unless --coverage-no-clean)
/// 3. Run tests (continuing even if tests fail to still generate report)
/// 4. Generate Cobertura XML report for CI integration (unless --coverage-no-report)
///
/// Requires `cargo-llvm-cov` to be installed: `cargo install cargo-llvm-cov`
fn run_tests_with_coverage(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
    coverage_args: &CoverageArgs,
    features: Option<&str>,
    brush_args: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let output_path = coverage_args.coverage_output.display().to_string();

    // Handle --coverage-report-only: just generate report from existing data
    if coverage_args.coverage_report_only {
        eprintln!("Generating coverage report from accumulated data...");
        if verbose {
            eprintln!("Running: cargo llvm-cov report --cobertura --output-path {output_path}");
        }
        cmd!(
            sh,
            "cargo llvm-cov report --cobertura --output-path {output_path}"
        )
        .run()
        .context("Failed to generate coverage report")?;
        eprintln!("Coverage report written to: {output_path}");
        return Ok(());
    }

    eprintln!("Running tests with coverage ({profile:?} profile)...");
    eprintln!("Coverage output: {output_path}");

    // Set up llvm-cov environment
    eprintln!("Setting up llvm-cov environment...");
    if verbose {
        eprintln!("Running: cargo llvm-cov show-env --export-prefix");
    }
    let env_output = cmd!(sh, "cargo llvm-cov show-env --export-prefix")
        .read()
        .context("Failed to get llvm-cov environment. Is cargo-llvm-cov installed?")?;

    // Parse and set environment variables from llvm-cov output
    env_output
        .lines()
        .filter_map(|line| line.strip_prefix("export "))
        .filter_map(|rest| rest.split_once('='))
        .for_each(|(k, v)| sh.set_var(k, v.trim_matches(['"', '\''])));

    // Clean previous coverage data (unless --coverage-no-clean)
    if !coverage_args.coverage_no_clean {
        if verbose {
            eprintln!("Running: cargo llvm-cov clean --workspace");
        }
        cmd!(sh, "cargo llvm-cov clean --workspace")
            .run()
            .context("Failed to clean coverage data")?;
    }

    // Build cargo nextest args
    let mut test_args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];
    if profile == BuildProfile::Release {
        test_args.push("--release");
    }

    // Add filter expression if provided
    let filter_value = filter_expr.map(str::to_string);
    if let Some(ref value) = filter_value {
        test_args.push("-E");
        test_args.push(value);
    }

    // Add features if provided
    let features_value = features.map(str::to_string);
    if let Some(ref value) = features_value {
        test_args.push("--features");
        test_args.push(value);
    }

    if verbose {
        eprintln!("Running: cargo {}", test_args.join(" "));
    }

    // Set BRUSH_ARGS env var if provided
    let _env_guard = brush_args.map(|val| sh.push_env("BRUSH_ARGS", val));

    // Run tests - let output pass through naturally, but continue on failure to generate coverage report
    let test_result = cmd!(sh, "cargo {test_args...}").run();
    let test_failed = test_result.is_err();

    if test_failed {
        eprintln!("Tests failed, but continuing to generate coverage report...");
    }

    // Generate coverage report (unless --coverage-no-report)
    if coverage_args.coverage_no_report {
        eprintln!("Skipping coverage report generation (--coverage-no-report)");
        eprintln!("Coverage data accumulated. Run with --coverage-report-only to generate report.");
    } else {
        eprintln!("Generating coverage report...");
        if verbose {
            eprintln!("Running: cargo llvm-cov report --cobertura --output-path {output_path}");
        }
        cmd!(
            sh,
            "cargo llvm-cov report --cobertura --output-path {output_path}"
        )
        .run()
        .context("Failed to generate coverage report")?;

        eprintln!("Coverage report written to: {output_path}");
    }

    // Now propagate test failure if tests failed
    if test_failed {
        anyhow::bail!("Tests failed (coverage report was still generated)");
    }

    eprintln!("Tests with coverage completed successfully.");
    Ok(())
}

/// List available bash-completion tests without running them.
fn list_bash_completion_tests(sh: &Shell, args: &BashCompletionArgs, verbose: bool) -> Result<()> {
    eprintln!("Collecting bash-completion tests...");

    // Determine test targets - specific files or all tests
    let test_targets: Vec<String> = if args.file.is_empty() {
        vec!["./t".to_string()]
    } else {
        args.file
            .iter()
            .map(|f| {
                if f.starts_with("./t/") || f.starts_with("t/") {
                    f.clone()
                } else {
                    format!("./t/{f}")
                }
            })
            .collect()
    };

    let mut pytest_args = vec!["--collect-only".to_string(), "-q".to_string()];

    // Add test filter if specified
    if let Some(filter) = &args.test_filter {
        pytest_args.push("-k".to_string());
        pytest_args.push(filter.clone());
    }

    // Add test targets
    pytest_args.extend(test_targets);

    if verbose {
        eprintln!("Running: pytest {}", pytest_args.join(" "));
    }

    // Run pytest --collect-only and display results
    cmd!(sh, "pytest").args(&pytest_args).run()?;

    Ok(())
}

/// Run the bash-completion project's test suite against brush.
///
/// This runs pytest on the bash-completion test suite with brush as the shell,
/// configured via the `BASH_COMPLETION_TEST_BASH` environment variable.
/// Results are output as JSON and optionally summarized to markdown.
///
/// Requires:
/// - A checkout of the bash-completion repository
/// - Python with pytest, pytest-xdist, and pytest-json-report installed
fn run_bash_completion_tests(
    sh: &Shell,
    args: &BashCompletionArgs,
    binary_args: &BinaryArgs,
    verbose: bool,
) -> Result<()> {
    // Find the brush binary (use explicit path or auto-detect from target dir)
    let brush_path = binary_args.find_brush_binary()?;

    let test_dir = args.bash_completion_path.join("test");
    if !test_dir.exists() {
        anyhow::bail!(
            "bash-completion test directory not found at: {}",
            test_dir.display()
        );
    }

    // Build the pytest command
    let dir_guard = sh.push_dir(&test_dir);

    // Set environment variable for the test suite
    let brush_path_str = brush_path.display().to_string();
    let _env = sh.push_env(
        "BASH_COMPLETION_TEST_BASH",
        format!("{brush_path_str} --noprofile --no-config --input-backend=basic"),
    );

    // Handle --list mode: just collect and display tests
    if args.list {
        return list_bash_completion_tests(sh, args, verbose);
    }

    eprintln!("Running bash-completion test suite...");
    eprintln!("Using brush binary: {}", brush_path.display());

    // Determine test targets - specific files or all tests
    let test_targets: Vec<String> = if args.file.is_empty() {
        vec!["./t".to_string()]
    } else {
        args.file
            .iter()
            .map(|f| {
                if f.starts_with("./t/") || f.starts_with("t/") {
                    f.clone()
                } else {
                    format!("./t/{f}")
                }
            })
            .collect()
    };

    // Build pytest args
    let mut pytest_args: Vec<String> = Vec::new();

    // Add parallel execution flag if jobs > 1 (requires pytest-xdist)
    if args.jobs > 1 {
        pytest_args.push("-n".to_string());
        pytest_args.push(args.jobs.to_string());
    }

    // Add JSON report if output is requested (requires pytest-json-report)
    let json_output = args.output.as_ref().map(|p| p.display().to_string());
    if let Some(ref output) = json_output {
        pytest_args.push("--json-report".to_string());
        pytest_args.push(format!("--json-report-file={output}"));
    }

    // Add optional flags
    if verbose {
        pytest_args.push("-v".to_string());
    }
    if args.stop_on_first {
        pytest_args.push("-x".to_string());
    }
    if let Some(filter) = &args.test_filter {
        pytest_args.push("-k".to_string());
        pytest_args.push(filter.clone());
    }

    // Add test targets at the end
    pytest_args.extend(test_targets);

    if verbose {
        eprintln!("Running: pytest {}", pytest_args.join(" "));
    }

    // Run pytest - pass stdout/stderr through directly, capture whether it failed.
    let pytest_failed = cmd!(sh, "pytest").args(&pytest_args).run().is_err();

    if pytest_failed {
        eprintln!("Some tests failed, but continuing to generate reports...");
    }

    // Generate summary report if requested (requires JSON output)
    if let (Some(summary_path), Some(output)) = (&args.summary_output, &json_output) {
        // Get workspace root for script path resolution
        let workspace_root = find_workspace_root()?;

        let summary_path_str = summary_path.display().to_string();

        // Determine the script path - use provided path or default to workspace root
        let script_path = args
            .summary_script
            .clone()
            .unwrap_or_else(|| workspace_root.join("scripts/summarize-pytest-results.py"));

        let script_path_str = script_path.display().to_string();

        // Go back to original directory for the script (if we were in test dir)
        drop(dir_guard);

        let title = "Test Summary: bash-completion test suite";
        if verbose {
            eprintln!("Running: python3 {script_path_str} -r {output} --title \"{title}\"");
        }
        let summary_result = cmd!(sh, "python3 {script_path_str}")
            .args(["-r", output, "--title", title])
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
    } else if args.summary_output.is_some() && json_output.is_none() {
        eprintln!("Warning: --summary-output requires --output for JSON results");
    }

    eprintln!("bash-completion test suite completed.");
    if let Some(ref output) = json_output {
        eprintln!("Results written to: {output}");
    }

    // Propagate test failure after reports are generated
    if pytest_failed {
        anyhow::bail!("bash-completion tests failed (reports were still generated)");
    }

    Ok(())
}
