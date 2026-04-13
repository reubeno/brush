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

use std::path::{Path, PathBuf};

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

#[cfg(windows)]
const TEST_BINARIES_DISABLED_ON_WINDOWS: &[&str] = &["brush-compat-tests"];

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

    /// Copy the nextest `JUnit` XML results to this path after the test run.
    /// The copy is performed even if tests fail, so CI can always upload results.
    #[clap(long)]
    pub results_output: Option<PathBuf>,

    /// Build and test against a wasm32-wasip2 target under a WASI runtime
    /// (wasmtime by default). Builds brush for wasm32-wasip2 with minimal
    /// features, then runs a subset of integration tests (excluding compat,
    /// interactive, and completion suites) under the WASI launcher.
    #[clap(long)]
    pub wasi: bool,

    /// Launcher command for the WASI runtime (only used with --wasi).
    /// The first token is resolved against `PATH`; subsequent tokens are
    /// passed as leading arguments before the brush binary path.
    /// Defaults to `wasmtime run --dir=.::/ --allow-precompiled --`.
    #[clap(long)]
    pub wasi_launcher: Option<String>,

    /// Skip the WASI wasm build step and assume brush.wasm is already
    /// present at the expected path (only used with --wasi).
    #[clap(long)]
    pub skip_wasi_build: bool,
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
            &args.coverage.coverage_output,
            verbose,
        )
    } else {
        run_nextest(sh, profile, Some(&filter_expr), verbose)?;
        eprintln!("Unit tests passed.");
        Ok(())
    }
}

/// Run all workspace tests (unit + integration).
///
/// This runs all tests in the workspace, including integration tests
/// that execute the brush binary. With `--wasi`, builds brush for
/// wasm32-wasip2 and runs the integration tests under a WASI runtime.
pub fn run_integration_tests(
    sh: &Shell,
    binary_args: &BinaryArgs,
    args: &IntegrationTestArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();

    if args.wasi {
        if args.coverage.coverage {
            eprintln!("Warning: --coverage is not supported with --wasi and will be ignored.");
        }
        return run_integration_tests_wasi(sh, profile, args, verbose);
    }

    eprintln!("Running integration tests ({profile:?} profile)...");

    #[cfg(windows)]
    let exclusions: Vec<String> = TEST_BINARIES_DISABLED_ON_WINDOWS
        .iter()
        .map(|name| format!("not binary({name})"))
        .collect();
    #[cfg(windows)]
    let filter_expr = exclusions.join(" and ");
    #[cfg(windows)]
    let filter = Some(filter_expr.as_str());

    #[cfg(not(windows))]
    let filter = None;

    let test_result = if args.coverage.coverage {
        run_tests_with_coverage(sh, profile, filter, &args.coverage.coverage_output, verbose)
    } else {
        run_nextest(sh, profile, filter, verbose).map(|()| {
            eprintln!("Integration tests passed.");
        })
    };

    // Copy nextest results if requested (even on test failure, so CI can upload them).
    if let Some(ref output) = args.results_output {
        copy_nextest_results(output)?;
    }

    test_result
}

/// Run the brush integration tests against a wasm32-wasip2 build of brush,
/// executed under a WASI runtime. Builds the wasm module first unless
/// `--skip-wasi-build` is given, then runs the integration tests via nextest
/// with the appropriate environment variables populated for the test harness.
fn run_integration_tests_wasi(
    sh: &Shell,
    profile: BuildProfile,
    args: &IntegrationTestArgs,
    verbose: bool,
) -> Result<()> {
    let is_release = profile == BuildProfile::Release;

    // Build the wasm module unless the caller opts out.
    if !args.skip_wasi_build {
        eprintln!("Building brush for wasm32-wasip2...");
        let mut build_args = vec![
            "build",
            "--target",
            "wasm32-wasip2",
            "-p",
            "brush-shell",
            "--bin",
            "brush",
            "--no-default-features",
            "--features",
            "minimal",
        ];
        if is_release {
            build_args.push("--release");
        }
        if verbose {
            eprintln!("Running: cargo {}", build_args.join(" "));
        }
        cmd!(sh, "cargo {build_args...}")
            .run()
            .context("failed to build brush for wasm32-wasip2")?;
    }

    // Locate the wasm module that was (or should have been) produced.
    let workspace_root = find_workspace_root()?;
    let profile_dir = if is_release { "release" } else { "debug" };
    let wasm_path = workspace_root
        .join("target/wasm32-wasip2")
        .join(profile_dir)
        .join("brush.wasm");
    let wasm_path = wasm_path.canonicalize().with_context(|| {
        format!(
            "brush.wasm not found at {} — did the build step succeed?",
            wasm_path.display()
        )
    })?;

    // The `--dir=.::/` flag maps the host root filesystem into the WASI
    // sandbox. This is intentionally permissive for testing — tests create
    // temp dirs and need access to fixtures across the filesystem. This is
    // NOT a recommended default for production use of brush under WASI.
    // Pre-compile the wasm module to avoid JIT compilation overhead during
    // parallel test execution. Without this, multiple concurrent wasmtime
    // processes each try to JIT-compile brush.wasm simultaneously, which
    // can exceed test timeouts on CI.
    eprintln!("Pre-compiling brush.wasm...");
    let cwasm_path = wasm_path.with_extension("cwasm");
    {
        let wasm_arg = wasm_path.display().to_string();
        let cwasm_arg = cwasm_path.display().to_string();
        cmd!(sh, "wasmtime compile {wasm_arg} -o {cwasm_arg}")
            .run()
            .context("failed to pre-compile brush.wasm with wasmtime")?;
    }

    // The default launcher includes --allow-precompiled so wasmtime accepts
    // the AOT-compiled .cwasm module without re-compilation.
    let launcher = args
        .wasi_launcher
        .as_deref()
        .unwrap_or("wasmtime run --dir=.::/ --allow-precompiled --");

    eprintln!("Running brush integration tests under WASI...");
    eprintln!("  wasm:     {}", cwasm_path.display());
    eprintln!("  launcher: {launcher}");

    let brush_path_str = cwasm_path.display().to_string();
    let _brush_path = sh.push_env("BRUSH_PATH", &brush_path_str);
    let _brush_launcher = sh.push_env("BRUSH_LAUNCHER", launcher);
    let _brush_platform_tags = sh.push_env("BRUSH_PLATFORM_TAGS", "wasi wasm");

    // Only run the brush integration tests; compat tests require a native binary.
    let filter = "binary(brush-integration-tests)";
    let test_result = run_nextest(sh, profile, Some(filter), verbose);

    // Copy nextest results if requested (even on test failure, so CI can upload them).
    if let Some(ref output) = args.results_output {
        copy_nextest_results(output)?;
    }

    test_result
}

/// Run cargo nextest with optional filter expression.
fn run_nextest(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
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

    if verbose {
        eprintln!("Running: cargo {}", args.join(" "));
    }

    cmd!(sh, "cargo {args...}").run().context("Tests failed")?;
    Ok(())
}

/// Copy the nextest `JUnit` XML results to the given output path.
fn copy_nextest_results(output: &Path) -> Result<()> {
    let workspace_root = find_workspace_root()?;
    let source = workspace_root.join("target/nextest/default/test-results.xml");
    std::fs::copy(&source, output).with_context(|| {
        format!(
            "Failed to copy nextest results from {} to {}",
            source.display(),
            output.display()
        )
    })?;
    eprintln!("Nextest results copied to: {}", output.display());
    Ok(())
}

/// Run tests with code coverage collection using `cargo-llvm-cov`.
///
/// The coverage workflow:
/// 1. Source environment variables from `cargo llvm-cov show-env`
/// 2. Clean previous coverage data
/// 3. Run tests (continuing even if tests fail to still generate report)
/// 4. Generate Cobertura XML report for CI integration
///
/// Requires `cargo-llvm-cov` to be installed: `cargo install cargo-llvm-cov`
fn run_tests_with_coverage(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
    output: &Path,
    verbose: bool,
) -> Result<()> {
    let output_path = output.display().to_string();

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

    // Clean previous coverage data
    if verbose {
        eprintln!("Running: cargo llvm-cov clean --workspace");
    }
    cmd!(sh, "cargo llvm-cov clean --workspace")
        .run()
        .context("Failed to clean coverage data")?;

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

    if verbose {
        eprintln!("Running: cargo {}", test_args.join(" "));
    }

    // Run tests - let output pass through naturally, but continue on failure to generate coverage
    // report
    let test_result = cmd!(sh, "cargo {test_args...}").run();
    let test_failed = test_result.is_err();

    if test_failed {
        eprintln!("Tests failed, but continuing to generate coverage report...");
    }

    // Generate coverage report (always attempt this)
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
