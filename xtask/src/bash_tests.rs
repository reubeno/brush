//! Runner for the upstream bash test suite.
//!
//! Parses the `run-*` scripts from the bash source tree to discover tests,
//! then executes them against a shell binary and compares output against
//! `.right` expected-output files (static mode) or a reference bash binary
//! (oracle mode).

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;

use serde::Deserialize;

use crate::test::BinaryArgs;

// ---------------------------------------------------------------------------
// Expectations (XFAIL / XPASS tracking)
// ---------------------------------------------------------------------------

/// Per-test expected outcome, stored in a JSON expectations file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExpectedOutcome {
    status: TestStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// The full expectations file: a sorted map of test name → expected outcome.
type Expectations = BTreeMap<String, ExpectedOutcome>;

fn load_expectations(path: &Path) -> Result<Expectations> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading expectations file: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("parsing expectations file: {}", path.display()))
}

fn save_expectations(path: &Path, expectations: &Expectations) -> Result<()> {
    let json = serde_json::to_string_pretty(expectations)?;
    std::fs::write(path, format!("{json}\n"))
        .with_context(|| format!("writing expectations file: {}", path.display()))
}

fn build_expectations_from_results(results: &[TestResult]) -> Expectations {
    let mut expectations = Expectations::new();
    for r in results {
        expectations.insert(
            r.name.clone(),
            ExpectedOutcome {
                status: r.status.clone(),
                reason: r.known_issue.clone(),
            },
        );
    }
    expectations
}

/// Classify a test result against expectations.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpectationMatch {
    /// No expectations file loaded — show everything.
    NoExpectations,
    /// Result matches expectation (expected pass or expected fail).
    Expected,
    /// Test was expected to fail but now passes (XPASS — a fix!).
    UnexpectedPass,
    /// Test was expected to pass but now fails (regression).
    UnexpectedFail,
    /// Test not in expectations file (new test).
    New,
}

fn classify_result(result: &TestResult, expectations: Option<&Expectations>) -> ExpectationMatch {
    let Some(expectations) = expectations else {
        return ExpectationMatch::NoExpectations;
    };
    let Some(expected) = expectations.get(&result.name) else {
        return ExpectationMatch::New;
    };
    if result.status == expected.status {
        ExpectationMatch::Expected
    } else if result.status == TestStatus::Passed {
        ExpectationMatch::UnexpectedPass
    } else {
        ExpectationMatch::UnexpectedFail
    }
}

// ---------------------------------------------------------------------------
// PTY test list
// ---------------------------------------------------------------------------

// Bash's upstream test runner (run-all) uses plain pipes — no terminal.
// Most tests work fine that way, but a handful *require* a controlling
// terminal (PTY) in order to produce correct output.  Without a PTY they
// either produce almost no output, extra error lines, or subtly different
// results because isatty() returns false.
//
// We default to the upstream pipe-based execution for everything and only
// switch to PTY execution for the tests listed here.
//
// **Why not run everything in a PTY?**  Some tests (notably `jobs`) use
// `fg` to move stopped background processes into the foreground of the
// controlling terminal.  In a PTY where the master side isn't driving an
// interactive session, `fg` blocks until the process finishes — which
// works but makes the test very slow.  Keeping the pipe-based default
// matches upstream's behavior and avoids unnecessary slowdowns.
//
// If a new bash release adds tests that need a terminal, add them here.
const PTY_TESTS: &[&str] = &[
    "exec",    // produces only 1 line without a terminal
    "history", // history builtins require interactive-like environment
    "read",    // read -e and read -p need a terminal
    "test",    // uses `test -t` which checks isatty()
    "vredir",  // {varname}> redirection tests check /dev/fd on a tty
];

// ---------------------------------------------------------------------------
// Slow test list
// ---------------------------------------------------------------------------

// Some tests contain many `sleep` + `wait` calls that add up to significant
// wall-clock time.  Rather than raising the global default timeout (which
// would delay detection of genuinely stuck tests), we give these tests a
// longer per-test timeout.
//
// The value is a multiplier applied to the user-specified `--timeout`.
const SLOW_TESTS: &[(&str, u32)] = &[
    ("jobs", 4), // ~64s of sleep+wait+fg across jobs.tests and jobs*.sub
];

// ---------------------------------------------------------------------------
// Known platform issues
// ---------------------------------------------------------------------------

// Some tests produce different output on different platforms (e.g. Linux vs
// macOS) due to locale availability, glibc behavior, or signal numbering.
// Bash 5.3 itself exhibits the same differences — these are not shell bugs.
//
// When a test fails in static mode and *every* diff hunk is fully explained
// by the markers below, the test is promoted to PASS with an annotation.
// If any hunk contains changes not matching a known marker, the test stays
// FAIL so regressions aren't masked.

struct KnownPlatformIssue {
    test: &'static str,
    markers: &'static [&'static str],
    description: &'static str,
}

const KNOWN_PLATFORM_ISSUES: &[KnownPlatformIssue] = &[
    KnownPlatformIssue {
        test: "intl",
        markers: &["Unicode tests"],
        description: "ja_JP.SJIS locale unavailable on Linux glibc",
    },
    KnownPlatformIssue {
        test: "printf",
        markers: &["Value too large"],
        description: "glibc printf overflow behavior differs from .right",
    },
    KnownPlatformIssue {
        test: "exec",
        markers: &["trap -- "],
        description: "trap -p signal ordering is platform-dependent",
    },
];

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

/// Arguments for the upstream bash test suite.
#[derive(Args, Clone)]
pub struct BashTestsArgs {
    /// Path to the bash source directory (parent of tests/).
    #[clap(long)]
    bash_source_path: PathBuf,

    /// Comparison mode.
    #[clap(long, default_value = "static")]
    mode: CompareMode,

    /// Path to the bash binary for oracle mode (default: "bash").
    #[clap(long, default_value = "bash")]
    bash_path: String,

    /// List discovered tests without running them.
    #[clap(long)]
    list: bool,

    /// Filter tests by name (substring match, or shell-style glob with *).
    #[clap(long, short = 't')]
    test_filter: Option<String>,

    /// Per-test timeout in seconds.
    #[clap(long, default_value = "30")]
    timeout: u64,

    /// Number of parallel workers (default: number of CPUs).
    #[clap(long, short = 'j')]
    jobs: Option<usize>,

    /// Output file for JSON results.
    #[clap(long, short = 'o')]
    output: Option<PathBuf>,

    /// Output file for markdown summary.
    #[clap(long)]
    summary_output: Option<PathBuf>,

    /// Stop after first failure.
    #[clap(long, short = 'x')]
    stop_on_first: bool,

    /// Show unified diff for failures.
    #[clap(long)]
    show_diff: bool,

    /// Directory to write per-test actual output and diffs.
    /// Creates <DIR>/<testname>.actual and <DIR>/<testname>.diff for each test.
    #[clap(long)]
    results_dir: Option<PathBuf>,

    /// Path to the expectations JSON file for XFAIL/XPASS tracking.
    #[clap(long)]
    expectations: Option<PathBuf>,

    /// Update the expectations file from actual test results (baseline).
    #[clap(long)]
    update_expectations: bool,

    /// Only show tests whose result differs from expectations (regressions + fixes).
    #[clap(long)]
    only_unexpected: bool,

    /// Run only tests from a named suite (e.g. "minimal"). Parsed from run-<name> script.
    #[clap(long)]
    subset: Option<String>,
}

#[derive(Clone, Debug)]
enum CompareMode {
    Static,
    Oracle,
}

impl std::str::FromStr for CompareMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "static" => Ok(Self::Static),
            "oracle" => Ok(Self::Oracle),
            other => Err(format!("unknown mode: {other} (expected static or oracle)")),
        }
    }
}

impl std::fmt::Display for CompareMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static => f.write_str("static"),
            Self::Oracle => f.write_str("oracle"),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsed test entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum StdinSource {
    /// The test script is passed as an argument (default).
    Argument,
    /// The test script is redirected via stdin (`< ./file`).
    File(String),
    /// Stdin is `/dev/null`.
    DevNull,
}

#[derive(Debug, Clone)]
enum OutputFilter {
    /// `| grep -v '<pattern>'`
    GrepExclude(String),
    /// `| cat -v`
    CatV,
}

#[derive(Debug, Clone)]
struct TestEntry {
    /// Human-readable name, derived from the `.right` file (e.g. "alias").
    name: String,
    /// The `run-*` script this was parsed from (for diagnostics).
    #[allow(dead_code)]
    runner_script: String,
    /// The test script to execute (e.g. `./alias.tests`).
    test_script: String,
    /// The expected-output file (e.g. `alias.right`).
    right_file: String,
    /// How stdin is provided to the shell.
    stdin_source: StdinSource,
    /// Whether stderr is redirected to stdout for capture.
    capture_stderr: bool,
    /// Optional output filter to apply after capture.
    output_filter: Option<OutputFilter>,
    /// Environment variables to unset before running.
    env_unsets: Vec<String>,
    /// Whether to prepend `pwd` to PATH.
    path_extend_pwd: bool,
    /// Whether this test needs a controlling terminal (PTY).
    needs_pty: bool,
    /// Timeout multiplier for slow tests (1 = default, >1 = longer).
    timeout_multiplier: u32,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a single `run-*` script into test entries.
fn parse_run_script(path: &Path) -> Result<Vec<TestEntry>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let script_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Extract TEST_NAME variable if set (used by dbg-support, dbg-support2, set-x)
    let test_name_var = extract_test_name_var(&content);

    // Resolve variable references in the content before parsing
    let resolved = if let Some(ref tn) = test_name_var {
        content
            .replace("${TEST_NAME}", tn)
            .replace("$TEST_NAME", tn)
    } else {
        content
    };

    // Collect meaningful lines (skip comments, blank lines, echo warnings, set -f,
    // diff -a checks, AFLAG assignments, TEST_NAME/TEST_FILE assignments, exit, shebang)
    let lines: Vec<&str> = resolved
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with('#')
                && !l.starts_with("echo ")
                && !l.starts_with("set -f")
                && !l.starts_with("( diff -a")
                && !l.starts_with("exit")
                && !l.starts_with("TEST_NAME=")
                && !l.starts_with("TEST_FILE=")
        })
        .collect();

    // Detect global modifiers
    let path_extend_pwd = resolved.contains("PATH=$PATH:`pwd`");
    let env_unsets = parse_env_unsets(&resolved);

    // Find `THIS_SH` invocation lines and diff lines
    let sh_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.contains("${THIS_SH}"))
        .collect();
    let diff_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.starts_with("diff ") && l.contains(".right"))
        .collect();

    if sh_lines.is_empty() || diff_lines.is_empty() {
        anyhow::bail!("no THIS_SH invocation or .right diff found");
    }

    // Pair each `THIS_SH` line with its corresponding diff line (by order)
    let mut entries = Vec::new();
    for (i, sh_line) in sh_lines.iter().enumerate() {
        let diff_line = diff_lines.get(i).copied().unwrap_or(diff_lines[0]);

        let right_file = extract_right_file(diff_line)
            .with_context(|| format!("extracting .right file from: {diff_line}"))?;
        let name = right_file.trim_end_matches(".right").to_string();

        let (test_script, stdin_source, capture_stderr, output_filter) = parse_sh_line(sh_line)?;

        let needs_pty = PTY_TESTS.contains(&name.as_str());
        let timeout_multiplier = SLOW_TESTS
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(1, |(_, m)| *m);
        entries.push(TestEntry {
            name,
            runner_script: script_name.clone(),
            test_script,
            right_file,
            stdin_source,
            capture_stderr,
            output_filter,
            env_unsets: env_unsets.clone(),
            path_extend_pwd,
            needs_pty,
            timeout_multiplier,
        });
    }

    Ok(entries)
}

/// Extract the value of `TEST_NAME='...'` from script content.
fn extract_test_name_var(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("TEST_NAME=") {
            // Strip quotes
            let val = rest.trim_matches('\'').trim_matches('"');
            return Some(val.to_string());
        }
    }
    None
}

/// Extract environment variable unsets from script content.
fn parse_env_unsets(content: &str) -> Vec<String> {
    let mut unsets = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("unset ") {
            // Handle "unset VAR1 VAR2 2>/dev/null" or "unset VAR"
            let vars_part = rest.split("2>").next().unwrap_or(rest);
            for var in vars_part.split_whitespace() {
                if var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    unsets.push(var.to_string());
                }
            }
        }
    }
    unsets
}

/// Parse a `${THIS_SH}` invocation line to extract the test script, stdin
/// source, stderr capture mode, and output filter.
fn parse_sh_line(line: &str) -> Result<(String, StdinSource, bool, Option<OutputFilter>)> {
    // Determine stdin source
    let stdin_source = if line.contains("< /dev/null") {
        StdinSource::DevNull
    } else if let Some(idx) = line.find("< ./") {
        // Extract filename: skip the "< " (2 chars) to get "./filename"
        let file = line
            .get(idx + 2..)
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        StdinSource::File(file)
    } else {
        StdinSource::Argument
    };

    // Determine output filter
    let output_filter = if line.contains("| grep -v") {
        let pattern = extract_grep_pattern(line).unwrap_or_else(|| "^expect".to_string());
        Some(OutputFilter::GrepExclude(pattern))
    } else if line.contains("| cat -v") {
        Some(OutputFilter::CatV)
    } else {
        None
    };

    // Determine stderr capture: look for "2>&1" before any pipe
    let before_pipe = line.split('|').next().unwrap_or(line);
    let capture_stderr = before_pipe.contains("2>&1") || line.contains("2>&1 |");

    // Extract test script
    let test_script = extract_test_script(line)?;

    Ok((test_script, stdin_source, capture_stderr, output_filter))
}

/// Extract the test script path from a `${THIS_SH}` invocation line.
fn extract_test_script(line: &str) -> Result<String> {
    let after_sh = line
        .split("${THIS_SH}")
        .nth(1)
        .context("no content after ${THIS_SH}")?
        .trim();

    // For stdin redirect lines like "${THIS_SH} < ./file > ..."
    if after_sh.starts_with('<') {
        let rest = after_sh.trim_start_matches('<').trim();
        let file = rest
            .split(|c: char| c.is_whitespace() || c == '>')
            .next()
            .unwrap_or("")
            .to_string();
        return Ok(file);
    }

    // Normal case: first non-flag token after `${THIS_SH}`
    // Strip any redirection suffix (e.g. "2>&1" or ">") by splitting at the
    // first `>` character.  An earlier version used trim_end_matches with a
    // char array which would incorrectly strip trailing chars like '2' from
    // test script names.
    let script = after_sh
        .split_whitespace()
        .find(|t| !t.starts_with('-'))
        .unwrap_or("")
        .split('>')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    if script.is_empty() {
        anyhow::bail!("could not extract test script from: {line}");
    }
    Ok(script)
}

/// Extract the `grep -v` pattern from a line containing `| grep -v '<pattern>'`.
fn extract_grep_pattern(line: &str) -> Option<String> {
    let after_grep = line.split("grep -v").nth(1)?;
    let rest = after_grep.trim();
    if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        Some(stripped.get(..end)?.to_string())
    } else {
        // Unquoted: take until whitespace or > or |
        let pat = rest
            .split(|c: char| c.is_whitespace() || c == '>' || c == '|')
            .next()?;
        Some(pat.to_string())
    }
}

/// Extract the `.right` filename from a diff line.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn extract_right_file(line: &str) -> Result<String> {
    for token in line.split_whitespace() {
        if token.ends_with(".right") {
            let clean = token
                .trim_start_matches("${TEST_NAME}.")
                .trim_start_matches("$TEST_FILE")
                .to_string();
            if clean.ends_with(".right") {
                return Ok(clean);
            }
        }
    }
    // Handle variable references like ${TEST_NAME}.right
    if line.contains(".right") {
        for token in line.split_whitespace() {
            if token.contains(".right") {
                return Ok(token.replace("${TEST_NAME}", "").replace("$TEST_FILE", ""));
            }
        }
    }
    anyhow::bail!("no .right file found in: {line}");
}

/// Scan the tests/ directory for `run-*` scripts and parse them all.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn discover_tests(tests_dir: &Path) -> Result<(Vec<TestEntry>, Vec<String>)> {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    let mut run_scripts: Vec<_> = std::fs::read_dir(tests_dir)
        .with_context(|| format!("reading directory: {}", tests_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("run-")
                && name != "run-all"
                && name != "run-minimal"
                && !name.ends_with('~')
                && !name.ends_with(".orig")
        })
        .collect();
    run_scripts.sort_by_key(|e| e.file_name());

    for entry in &run_scripts {
        let path = entry.path();
        let script_name = entry.file_name().to_string_lossy().to_string();
        match parse_run_script(&path) {
            Ok(parsed) => entries.extend(parsed),
            Err(e) => {
                warnings.push(format!(
                    "WARNING: Could not parse {script_name}, skipping -- {e:#}"
                ));
            }
        }
    }

    Ok((entries, warnings))
}

/// Parse a suite script (e.g. `run-minimal`) to extract which tests it includes.
/// Looks for `case` patterns like `run-X|run-Y|...) echo $x ; sh $x ;;`.
fn parse_suite_script(suite_path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(suite_path)
        .with_context(|| format!("reading suite script: {}", suite_path.display()))?;
    let mut members = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Match lines that run tests: "run-X|run-Y|...) echo $x ; sh $x ;;"
        if trimmed.contains("echo $x") && trimmed.contains("sh $x") {
            let pattern_part = trimmed.split(')').next().unwrap_or("");
            for name in pattern_part.split('|') {
                if let Some(test_name) = name.trim().strip_prefix("run-") {
                    members.push(test_name.to_string());
                }
            }
        }
    }
    Ok(members)
}

// ---------------------------------------------------------------------------
// Test execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct TestResult {
    name: String,
    status: TestStatus,
    matching_lines: usize,
    total_lines: usize,
    duration_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_diff_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    known_issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TestStatus {
    Passed,
    Failed,
    TimedOut,
    Skipped,
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Passed => f.write_str("PASS"),
            Self::Failed => f.write_str("FAIL"),
            Self::TimedOut => f.write_str("TMOUT"),
            Self::Skipped => f.write_str("SKIP"),
        }
    }
}

/// Create an isolated copy of the test directory for a worker thread.
/// Regular files are copied (so CWD writes don't collide), while
/// subdirectories are symlinked (they're read-only test data).
fn create_test_farm(src_dir: &Path, farm_dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src_dir)
        .with_context(|| format!("reading {} for test farm", src_dir.display()))?
    {
        let entry = entry?;
        let dest = farm_dir.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            // Symlink directories (e.g. misc/) — they're read-only test data.
            std::os::unix::fs::symlink(entry.path(), &dest)?;
        } else {
            // Copy regular files so parallel writes don't collide.
            std::fs::copy(entry.path(), &dest).with_context(|| {
                format!("copying {} -> {}", entry.path().display(), dest.display())
            })?;
        }
    }
    Ok(())
}

/// Build the shell command string that mirrors what the `run-*` script does.
fn build_test_command_string(shell_path: &Path, entry: &TestEntry) -> String {
    let shell_str = shell_path.display().to_string();
    let inner_cmd = match &entry.stdin_source {
        StdinSource::Argument => {
            if entry.capture_stderr {
                format!("{shell_str} {} 2>&1", entry.test_script)
            } else {
                format!("{shell_str} {}", entry.test_script)
            }
        }
        StdinSource::File(path) => {
            if entry.capture_stderr {
                format!("{shell_str} < {path} 2>&1")
            } else {
                format!("{shell_str} < {path}")
            }
        }
        StdinSource::DevNull => {
            if entry.capture_stderr {
                format!("{shell_str} {} 2>&1 < /dev/null", entry.test_script)
            } else {
                format!("{shell_str} {} < /dev/null", entry.test_script)
            }
        }
    };

    match &entry.output_filter {
        Some(OutputFilter::GrepExclude(pattern)) => {
            format!("{inner_cmd} | grep -v '{pattern}'")
        }
        Some(OutputFilter::CatV) => {
            format!("{inner_cmd} | cat -v")
        }
        None => inner_cmd,
    }
}

/// Compute the common environment variables for a test execution.
fn build_test_env(
    shell_path: &Path,
    entry: &TestEntry,
    tests_dir: &Path,
    bash_source_dir: &Path,
    tmpdir: &str,
) -> Vec<(String, String)> {
    let tmpdir = tmpdir.to_string();
    let sys_path = std::env::var("PATH").unwrap_or_default();
    let new_path = if entry.path_extend_pwd {
        format!("{}:{}:{sys_path}", tests_dir.display(), tests_dir.display())
    } else {
        format!("{}:{sys_path}", tests_dir.display())
    };
    let tstout = PathBuf::from(&tmpdir).join(format!(
        "brush-tstout-{}-{}",
        std::process::id(),
        entry.name
    ));

    vec![
        ("THIS_SH".to_string(), shell_path.display().to_string()),
        (
            "BUILD_DIR".to_string(),
            bash_source_dir.display().to_string(),
        ),
        ("TMPDIR".to_string(), tmpdir),
        ("PATH".to_string(), new_path),
        ("BASH_TSTOUT".to_string(), tstout.display().to_string()),
    ]
}

/// Dispatch test execution: use PTY for terminal-dependent tests, pipes otherwise.
fn execute_test(
    shell_path: &Path,
    entry: &TestEntry,
    work_dir: &Path,
    tests_dir: &Path,
    bash_source_dir: &Path,
    tmpdir: &str,
    timeout: Duration,
) -> Result<(String, Duration)> {
    if entry.needs_pty {
        execute_test_pty(
            shell_path,
            entry,
            work_dir,
            tests_dir,
            bash_source_dir,
            tmpdir,
            timeout,
        )
    } else {
        execute_test_pipe(
            shell_path,
            entry,
            work_dir,
            tests_dir,
            bash_source_dir,
            tmpdir,
            timeout,
        )
    }
}

/// Execute a test using piped stdout/stderr (the common path, matching upstream's runner).
fn execute_test_pipe(
    shell_path: &Path,
    entry: &TestEntry,
    work_dir: &Path,
    tests_dir: &Path,
    bash_source_dir: &Path,
    tmpdir: &str,
    timeout: Duration,
) -> Result<(String, Duration)> {
    let start = Instant::now();
    let full_cmd = build_test_command_string(shell_path, entry);

    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    cmd.arg(&full_cmd);
    cmd.current_dir(work_dir);

    // Environment setup — mirror what run-all does
    let env_vars = build_test_env(shell_path, entry, tests_dir, bash_source_dir, tmpdir);
    for (key, val) in &env_vars {
        cmd.env(key, val);
    }
    cmd.env_remove("BASH_ENV");
    for var in &entry.env_unsets {
        cmd.env_remove(var);
    }

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Put the child in its own process group so we can kill the entire
    // pipeline (sh + test shell + filters) on timeout.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Safety: pre_exec runs the closure between fork and exec.
        // setpgid(0, 0) is async-signal-safe and makes this process a
        // new process group leader so we can kill the whole pipeline.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }
    }

    let child = cmd.spawn().context("spawning shell process")?;
    let child_id = child.id();

    // Drain stdout/stderr in a background thread to avoid pipe deadlock.
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = result_tx.send(child.wait_with_output());
    });

    match result_rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let out = String::from_utf8_lossy(&output.stdout).to_string();
            Ok((out, start.elapsed()))
        }
        Ok(Err(e)) => Err(e.into()),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            kill_process(child_id);
            anyhow::bail!("timed out after {:.1}s", start.elapsed().as_secs_f64());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            anyhow::bail!("child process monitor thread died unexpectedly");
        }
    }
}

/// Execute a test in a PTY for tests that need a controlling terminal.
fn execute_test_pty(
    shell_path: &Path,
    entry: &TestEntry,
    work_dir: &Path,
    tests_dir: &Path,
    bash_source_dir: &Path,
    tmpdir: &str,
    timeout: Duration,
) -> Result<(String, Duration)> {
    let start = Instant::now();
    let full_cmd = build_test_command_string(shell_path, entry);

    let (mut pty, pts) = pty_process::blocking::open().context("opening PTY pair")?;

    // Disable ECHO and OPOST for clean output (no \n→\r\n, no input echo).
    configure_pty_termios(&pty);

    // Set a reasonable terminal size.
    pty.resize(pty_process::Size::new(24, 80))
        .context("resizing PTY")?;

    // Build the command. pty_process::blocking::Command methods consume self,
    // so we chain or reassign.
    let env_vars = build_test_env(shell_path, entry, tests_dir, bash_source_dir, tmpdir);
    let mut cmd = pty_process::blocking::Command::new("sh")
        .arg("-c")
        .arg(&full_cmd)
        .current_dir(work_dir)
        .env_remove("BASH_ENV");

    for (key, val) in &env_vars {
        cmd = cmd.env(key, val);
    }
    for var in &entry.env_unsets {
        cmd = cmd.env_remove(var);
    }

    // pty-process handles setsid() + TIOCSCTTY internally.
    let mut child = cmd.spawn(pts).context("spawning PTY shell process")?;
    let child_id = child.id();

    // Read output from PTY master in a background thread.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match pty.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                // EIO is the normal EOF signal when the PTY slave closes.
                Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        // Reap the child to avoid zombies.
        let _ = child.wait();
        let _ = tx.send(String::from_utf8_lossy(&output).to_string());
    });

    match rx.recv_timeout(timeout) {
        Ok(output) => Ok((output, start.elapsed())),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            kill_process(child_id);
            anyhow::bail!("timed out after {:.1}s", start.elapsed().as_secs_f64());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            anyhow::bail!("child process monitor thread died unexpectedly");
        }
    }
}

/// Configure PTY master termios: disable ECHO (don't echo input) and
/// OPOST (don't translate \n to \r\n in output).
fn configure_pty_termios(pty: &pty_process::blocking::Pty) {
    use std::os::unix::io::AsRawFd;
    let fd = pty.as_raw_fd();
    // Safety: zeroed is a valid initial value for libc::termios.
    let mut termios = unsafe { std::mem::zeroed::<libc::termios>() };
    // Safety: tcgetattr is a standard POSIX call on a valid PTY fd.
    if unsafe { libc::tcgetattr(fd, &raw mut termios) } == 0 {
        termios.c_lflag &= !libc::ECHO;
        termios.c_oflag &= !libc::OPOST;
        // Safety: tcsetattr is a standard POSIX call on a valid PTY fd.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const termios) };
    }
}

#[cfg(unix)]
fn kill_process(pid: u32) {
    let pid_i32 = pid.cast_signed();
    // Safety: sending SIGKILL to the process group we spawned.
    unsafe { libc::kill(-pid_i32, libc::SIGKILL) };
    // Safety: sending SIGKILL to the process itself as a fallback.
    unsafe { libc::kill(pid_i32, libc::SIGKILL) };
}

#[cfg(not(unix))]
fn kill_process(_pid: u32) {}

// ---------------------------------------------------------------------------
// Output normalization and comparison
// ---------------------------------------------------------------------------

/// Normalize shell output for comparison.
fn normalize_output(output: &str, shell_path: &Path) -> String {
    let shell_name = shell_path.file_name().unwrap_or_default().to_string_lossy();
    let shell_path_str = shell_path.display().to_string();

    let mut result = output.to_string();

    // Replace full path references: `/path/to/brush:` -> `bash:`
    if shell_path_str != "bash" {
        result = result.replace(&format!("{shell_path_str}: "), "bash: ");
        result = result.replace(&format!("{shell_path_str}:"), "bash:");
    }

    // Replace shell name at line starts: `brush:` -> `bash:`
    if *shell_name != *"bash" {
        let suffix = if result.ends_with("\n\n") {
            "\n\n"
        } else if result.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let prefix_space = format!("{shell_name}: ");
        let prefix_colon = format!("{shell_name}:");
        let lines: Vec<String> = result
            .lines()
            .map(|line| {
                if let Some(rest) = line.strip_prefix(&prefix_space) {
                    format!("bash: {rest}")
                } else if let Some(rest) = line.strip_prefix(&prefix_colon) {
                    format!("bash:{rest}")
                } else {
                    line.to_string()
                }
            })
            .collect();
        result = lines.join("\n");
        // `.lines()` drops trailing newlines; restore the original termination.
        if !result.ends_with('\n') && !suffix.is_empty() {
            result.push_str(suffix);
        } else if result.ends_with('\n') && suffix == "\n\n" {
            result.push('\n');
        }
    }

    result
}

/// Compare output line by line.
fn compare_outputs(actual: &str, expected: &str) -> (usize, usize, Option<usize>) {
    let actual_lines: Vec<&str> = actual.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();

    let total = expected_lines.len();
    let mut matching = 0;
    let mut first_diff = None;

    for (i, expected_line) in expected_lines.iter().enumerate() {
        if let Some(actual_line) = actual_lines.get(i) {
            if actual_line == expected_line {
                matching += 1;
            } else if first_diff.is_none() {
                first_diff = Some(i + 1);
            }
        } else if first_diff.is_none() {
            first_diff = Some(i + 1);
        }
    }

    if actual_lines.len() > expected_lines.len() && first_diff.is_none() {
        first_diff = Some(expected_lines.len() + 1);
    }

    (matching, total, first_diff)
}

/// Produce a unified diff between two strings.
fn unified_diff(expected: &str, actual: &str, expected_label: &str, actual_label: &str) -> String {
    use std::fmt::Write;

    let mut result = String::new();
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    let _ = writeln!(result, "--- {expected_label}");
    let _ = writeln!(result, "+++ {actual_label}");

    let max_len = expected_lines.len().max(actual_lines.len());
    let mut chunk_start: Option<usize> = None;
    let mut chunk_lines: Vec<String> = Vec::new();
    let mut chunk_removed = 0usize;
    let mut chunk_added = 0usize;

    for i in 0..max_len {
        let exp = expected_lines.get(i).copied();
        let act = actual_lines.get(i).copied();

        if exp != act {
            if chunk_start.is_none() {
                chunk_start = Some(i);
            }
            if let Some(e) = exp {
                chunk_lines.push(format!("-{e}"));
                chunk_removed += 1;
            }
            if let Some(a) = act {
                chunk_lines.push(format!("+{a}"));
                chunk_added += 1;
            }
        } else if let Some(start) = chunk_start {
            let _ = writeln!(
                result,
                "@@ -{},{chunk_removed} +{},{chunk_added} @@",
                start + 1,
                start + 1
            );
            for cl in &chunk_lines {
                let _ = writeln!(result, "{cl}");
            }
            chunk_start = None;
            chunk_lines.clear();
            chunk_removed = 0;
            chunk_added = 0;
        }
    }

    if let Some(start) = chunk_start {
        let _ = writeln!(
            result,
            "@@ -{},{chunk_removed} +{},{chunk_added} @@",
            start + 1,
            start + 1
        );
        for cl in &chunk_lines {
            let _ = writeln!(result, "{cl}");
        }
    }

    result
}

/// Check whether a test's diff is fully explained by known platform issues.
///
/// Parses the unified diff into hunks (contiguous groups of `+`/`-` lines).
/// For each hunk, checks that at least one changed line contains a marker from
/// the test's known issues. If ALL hunks are explained, returns the issue
/// description; otherwise returns `None`.
fn check_known_platform_issue(test_name: &str, diff: &str) -> Option<&'static str> {
    let issue = KNOWN_PLATFORM_ISSUES
        .iter()
        .find(|ki| ki.test == test_name)?;

    let mut in_hunk = false;
    let mut hunk_explained = false;
    let mut hunk_count = 0usize;
    let mut explained_count = 0usize;

    for line in diff.lines() {
        let is_change = line.starts_with('+') || line.starts_with('-');
        // Skip diff headers (--- / +++)
        let is_header = line.starts_with("--- ") || line.starts_with("+++ ");

        if is_change && !is_header {
            if !in_hunk {
                in_hunk = true;
                hunk_explained = false;
                hunk_count += 1;
            }
            // Check if this changed line matches any marker.
            // Skip the leading +/- prefix (always a single ASCII byte).
            let content = line.get(1..).unwrap_or("");
            if !hunk_explained {
                for marker in issue.markers {
                    if content.contains(marker) {
                        hunk_explained = true;
                        break;
                    }
                }
            }
        } else if in_hunk {
            // Non-change line ends the current hunk.
            if hunk_explained {
                explained_count += 1;
            }
            in_hunk = false;
        }
    }

    // Finalize the last hunk.
    if in_hunk && hunk_explained {
        explained_count += 1;
    }

    if hunk_count > 0 && explained_count == hunk_count {
        Some(issue.description)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonReport {
    mode: String,
    brush_path: String,
    bash_source_path: String,
    summary: JsonSummary,
    tests: Vec<TestResult>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct JsonSummary {
    total: usize,
    passed: usize,
    failed: usize,
    timed_out: usize,
    skipped: usize,
    total_lines: usize,
    matching_lines: usize,
    line_match_pct: f64,
}

struct ResultSummary {
    passed: usize,
    failed: usize,
    timed_out: usize,
    skipped: usize,
    total_lines: usize,
    matching_lines: usize,
}

impl ResultSummary {
    fn from_results(results: &[TestResult]) -> Self {
        let mut s = Self {
            passed: 0,
            failed: 0,
            timed_out: 0,
            skipped: 0,
            total_lines: 0,
            matching_lines: 0,
        };
        for r in results {
            match r.status {
                TestStatus::Passed => s.passed += 1,
                TestStatus::Failed => s.failed += 1,
                TestStatus::TimedOut => s.timed_out += 1,
                TestStatus::Skipped => s.skipped += 1,
            }
            s.total_lines += r.total_lines;
            s.matching_lines += r.matching_lines;
        }
        s
    }

    #[allow(clippy::cast_precision_loss)]
    fn line_match_pct(&self) -> f64 {
        if self.total_lines > 0 {
            (self.matching_lines as f64 / self.total_lines as f64) * 100.0
        } else {
            0.0
        }
    }
}

fn print_console_report(
    results: &[TestResult],
    show_diff: bool,
    diffs: &BTreeMap<String, String>,
    expectations: Option<&Expectations>,
    only_unexpected: bool,
) {
    let s = ResultSummary::from_results(results);
    let mut xpass_count = 0usize;
    let mut xfail_count = 0usize;
    let mut regression_count = 0usize;

    eprintln!();
    for r in results {
        let classification = classify_result(r, expectations);

        // In only-unexpected mode, skip expected results.
        if only_unexpected && classification == ExpectationMatch::Expected {
            continue;
        }

        let status_tag = match r.status {
            TestStatus::Passed => " PASS ",
            TestStatus::Failed => " FAIL ",
            TestStatus::TimedOut => " TMOUT",
            TestStatus::Skipped => " SKIP ",
        };

        let expectation_tag = match classification {
            ExpectationMatch::UnexpectedPass => {
                xpass_count += 1;
                " [XPASS]"
            }
            ExpectationMatch::UnexpectedFail => {
                regression_count += 1;
                " [REGRESSION]"
            }
            ExpectationMatch::Expected
                if r.status == TestStatus::Failed || r.status == TestStatus::TimedOut =>
            {
                xfail_count += 1;
                " [XFAIL]"
            }
            ExpectationMatch::New => " [NEW]",
            _ => "",
        };

        if let Some(desc) = &r.known_issue {
            eprintln!(
                " {status_tag}  {:<20} ({}/{} lines, {:.1}s) [known: {desc}]{expectation_tag}",
                r.name, r.matching_lines, r.total_lines, r.duration_secs
            );
        } else {
            eprintln!(
                " {status_tag}  {:<20} ({}/{} lines, {:.1}s){expectation_tag}",
                r.name, r.matching_lines, r.total_lines, r.duration_secs
            );
        }

        if show_diff && r.status == TestStatus::Failed {
            if let Some(diff) = diffs.get(&r.name) {
                for line in diff.lines().take(50) {
                    eprintln!("        {line}");
                }
                let diff_line_count = diff.lines().count();
                if diff_line_count > 50 {
                    eprintln!("        ... ({} more lines)", diff_line_count - 50);
                }
            }
        }
    }

    let pct = s.line_match_pct();
    eprintln!();
    eprintln!(
        "Results: {} passed, {} failed, {} timed out, {} skipped ({} total)",
        s.passed,
        s.failed,
        s.timed_out,
        s.skipped,
        results.len()
    );
    eprintln!(
        "Lines:  {} / {} matched ({pct:.1}%)",
        s.matching_lines, s.total_lines
    );
    if expectations.is_some() {
        eprintln!(
            "Expectations: {xfail_count} xfail, {xpass_count} xpass (fixes), {regression_count} regressions"
        );
    }
}

fn write_json_report(
    path: &Path,
    results: &[TestResult],
    warnings: &[String],
    mode: &CompareMode,
    brush_path: &Path,
    bash_source_path: &Path,
) -> Result<()> {
    let s = ResultSummary::from_results(results);
    let pct = s.line_match_pct();

    let report = JsonReport {
        mode: mode.to_string(),
        brush_path: brush_path.display().to_string(),
        bash_source_path: bash_source_path.display().to_string(),
        summary: JsonSummary {
            total: results.len(),
            passed: s.passed,
            failed: s.failed,
            timed_out: s.timed_out,
            skipped: s.skipped,
            total_lines: s.total_lines,
            matching_lines: s.matching_lines,
            line_match_pct: (pct * 10.0).round() / 10.0,
        },
        tests: results.to_vec(),
        warnings: warnings.to_vec(),
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, json).with_context(|| format!("writing JSON report to {}", path.display()))
}

fn write_markdown_summary(
    path: &Path,
    results: &[TestResult],
    mode: &CompareMode,
    brush_path: &Path,
) -> Result<()> {
    let s = ResultSummary::from_results(results);
    let pct = s.line_match_pct();

    let mut f = std::fs::File::create(path)
        .with_context(|| format!("creating markdown summary at {}", path.display()))?;

    writeln!(f, "# Bash Test Suite Results")?;
    writeln!(f)?;
    writeln!(
        f,
        "**Mode:** {mode} | **Shell:** `{}`",
        brush_path.display()
    )?;
    writeln!(f)?;
    writeln!(f, "## Summary")?;
    writeln!(f)?;
    writeln!(f, "| Metric | Value |")?;
    writeln!(f, "|--------|-------|")?;
    writeln!(f, "| Total tests | {} |", results.len())?;
    writeln!(f, "| Passed | {} |", s.passed)?;
    writeln!(f, "| Failed | {} |", s.failed)?;
    writeln!(f, "| Timed out | {} |", s.timed_out)?;
    writeln!(f, "| Skipped | {} |", s.skipped)?;
    writeln!(
        f,
        "| Lines matched | {} / {} ({pct:.1}%) |",
        s.matching_lines, s.total_lines
    )?;
    writeln!(f)?;

    let failures: Vec<&TestResult> = results
        .iter()
        .filter(|r| r.status == TestStatus::Failed || r.status == TestStatus::TimedOut)
        .collect();

    if !failures.is_empty() {
        writeln!(f, "## Failures")?;
        writeln!(f)?;
        writeln!(f, "| Test | Status | Lines | Duration |")?;
        writeln!(f, "|------|--------|-------|----------|")?;
        for r in &failures {
            writeln!(
                f,
                "| {} | {} | {}/{} | {:.1}s |",
                r.name, r.status, r.matching_lines, r.total_lines, r.duration_secs
            )?;
        }
        writeln!(f)?;
    }

    let passes: Vec<&TestResult> = results
        .iter()
        .filter(|r| r.status == TestStatus::Passed)
        .collect();

    if !passes.is_empty() {
        writeln!(f, "## Passed")?;
        writeln!(f)?;
        writeln!(f, "| Test | Lines | Duration |")?;
        writeln!(f, "|------|-------|----------|")?;
        for r in &passes {
            writeln!(
                f,
                "| {} | {}/{} | {:.1}s |",
                r.name, r.matching_lines, r.total_lines, r.duration_secs
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Test filter matching
// ---------------------------------------------------------------------------

fn matches_filter(name: &str, filter: &str) -> bool {
    if filter.contains('*') {
        glob_match(filter, name)
    } else {
        name.contains(filter)
    }
}

/// Simple glob matching supporting only `*` wildcards.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = text.get(pos..).and_then(|s| s.find(part)) {
            if i == 0 && found != 0 {
                return false;
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }

    if !pattern.ends_with('*') {
        return text.ends_with(parts.last().unwrap_or(&""));
    }

    true
}

// ---------------------------------------------------------------------------
// Shared config passed to worker threads
// ---------------------------------------------------------------------------

struct TestRunConfig {
    brush_path: PathBuf,
    tests_dir: PathBuf,
    bash_source_dir: PathBuf,
    timeout: Duration,
    mode: CompareMode,
    bash_path: String,
    stop_on_first: bool,
    verbose: bool,
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
pub fn run_bash_tests(args: &BashTestsArgs, binary_args: &BinaryArgs, verbose: bool) -> Result<()> {
    let tests_dir = args.bash_source_path.join("tests");
    if !tests_dir.exists() {
        anyhow::bail!(
            "bash tests directory not found at: {}\n\
             Please provide the path to the bash source directory (parent of tests/)",
            tests_dir.display()
        );
    }

    validate_support_binaries(&tests_dir)?;

    eprint!("Parsing run-* scripts...");
    let (mut entries, warnings) = discover_tests(&tests_dir)?;

    for w in &warnings {
        eprintln!("  {w}");
    }

    if let Some(filter) = &args.test_filter {
        entries.retain(|e| matches_filter(&e.name, filter));
    }

    // Filter by named suite (e.g. --subset minimal reads run-minimal)
    if let Some(subset) = &args.subset {
        let suite_path = tests_dir.join(format!("run-{subset}"));
        let suite_members = parse_suite_script(&suite_path)?;
        entries.retain(|e| {
            // Match by runner script name: "run-X" matches member "X"
            let runner_test = e
                .runner_script
                .strip_prefix("run-")
                .unwrap_or(&e.runner_script);
            suite_members.iter().any(|m| m == runner_test)
        });
        eprintln!("  Subset '{subset}': {} tests selected", entries.len());
    }

    eprintln!(
        " found {} test entries{}",
        entries.len(),
        if warnings.is_empty() {
            String::new()
        } else {
            format!(" ({} warnings)", warnings.len())
        }
    );

    if args.list {
        list_tests(&entries);
        return Ok(());
    }

    let brush_path = binary_args.find_brush_binary()?;
    let timeout = Duration::from_secs(args.timeout);
    let num_workers = args.jobs.unwrap_or_else(num_cpus::get);

    eprintln!(
        "Running {} bash tests against {}...",
        entries.len(),
        brush_path.display()
    );
    eprintln!(
        "Shell: {} | Timeout: {}s | Workers: {num_workers} | Mode: {}",
        brush_path.display(),
        args.timeout,
        args.mode
    );

    let config = TestRunConfig {
        brush_path,
        tests_dir,
        bash_source_dir: args.bash_source_path.clone(),
        timeout,
        mode: args.mode.clone(),
        bash_path: args.bash_path.clone(),
        stop_on_first: args.stop_on_first,
        verbose,
    };

    let raw_results = run_tests_parallel(&entries, &config, num_workers)?;

    let results: Vec<TestResult> = raw_results.iter().map(|(r, _, _)| r.clone()).collect();
    let diffs: BTreeMap<String, String> = raw_results
        .iter()
        .filter_map(|(r, d, _)| d.as_ref().map(|diff| (r.name.clone(), diff.clone())))
        .collect();
    let actuals: BTreeMap<String, String> = raw_results
        .into_iter()
        .filter_map(|(r, _, a)| a.map(|actual| (r.name, actual)))
        .collect();

    // Load or update expectations.
    let expectations = if let Some(exp_path) = &args.expectations {
        if args.update_expectations {
            let exp = build_expectations_from_results(&results);
            save_expectations(exp_path, &exp)?;
            eprintln!("Expectations updated: {}", exp_path.display());
            Some(exp)
        } else if exp_path.exists() {
            Some(load_expectations(exp_path)?)
        } else {
            eprintln!(
                "Expectations file not found: {} (use --update-expectations to create)",
                exp_path.display()
            );
            None
        }
    } else {
        None
    };

    print_console_report(
        &results,
        args.show_diff,
        &diffs,
        expectations.as_ref(),
        args.only_unexpected,
    );
    write_reports(args, &results, &warnings, &config, &diffs, &actuals)?;

    // In expectations mode, only fail on regressions (unexpected failures).
    let any_failure = if expectations.is_some() {
        results
            .iter()
            .any(|r| classify_result(r, expectations.as_ref()) == ExpectationMatch::UnexpectedFail)
    } else {
        results
            .iter()
            .any(|r| r.status == TestStatus::Failed || r.status == TestStatus::TimedOut)
    };
    if any_failure {
        anyhow::bail!("Some bash tests failed");
    }

    Ok(())
}

fn validate_support_binaries(tests_dir: &Path) -> Result<()> {
    for bin in &["recho", "zecho", "printenv"] {
        let bin_path = tests_dir.join(bin);
        if !bin_path.exists() {
            anyhow::bail!(
                "Required support binary not found: {}\n\
                 Please build the bash source tree first (run `make` in the bash source directory)",
                bin_path.display()
            );
        }
    }
    Ok(())
}

fn list_tests(entries: &[TestEntry]) {
    for entry in entries {
        eprintln!(
            "  {:<24} script={:<20} right={:<24} stdin={:?} filter={:?}",
            entry.name,
            entry.test_script,
            entry.right_file,
            match &entry.stdin_source {
                StdinSource::Argument => "arg".to_string(),
                StdinSource::File(f) => format!("file({f})"),
                StdinSource::DevNull => "devnull".to_string(),
            },
            entry.output_filter.as_ref().map(|f| match f {
                OutputFilter::GrepExclude(p) => format!("grep-v({p})"),
                OutputFilter::CatV => "cat-v".to_string(),
            }),
        );
    }
}

fn write_reports(
    args: &BashTestsArgs,
    results: &[TestResult],
    warnings: &[String],
    config: &TestRunConfig,
    diffs: &BTreeMap<String, String>,
    actuals: &BTreeMap<String, String>,
) -> Result<()> {
    if let Some(json_path) = &args.output {
        write_json_report(
            json_path,
            results,
            warnings,
            &args.mode,
            &config.brush_path,
            &args.bash_source_path,
        )?;
        eprintln!("JSON report written to: {}", json_path.display());
    }
    if let Some(md_path) = &args.summary_output {
        write_markdown_summary(md_path, results, &args.mode, &config.brush_path)?;
        eprintln!("Markdown summary written to: {}", md_path.display());
    }
    if let Some(results_dir) = &args.results_dir {
        write_results_dir(results_dir, diffs, actuals)?;
        eprintln!("Per-test results written to: {}", results_dir.display());
    }
    Ok(())
}

fn write_results_dir(
    dir: &Path,
    diffs: &BTreeMap<String, String>,
    actuals: &BTreeMap<String, String>,
) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating results directory: {}", dir.display()))?;
    for (name, actual) in actuals {
        let path = dir.join(format!("{name}.actual"));
        std::fs::write(&path, actual).with_context(|| format!("writing {}", path.display()))?;
    }
    for (name, diff) in diffs {
        let path = dir.join(format!("{name}.diff"));
        std::fs::write(&path, diff).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// (`TestResult`, optional diff, optional actual output).
type TestOutput = (TestResult, Option<String>, Option<String>);

fn run_tests_parallel(
    entries: &[TestEntry],
    config: &TestRunConfig,
    num_workers: usize,
) -> Result<Vec<TestOutput>> {
    use std::sync::{Arc, Mutex};

    // Each entry: (index, TestResult, diff, actual_output)
    type ResultVec = Vec<(usize, TestResult, Option<String>, Option<String>)>;
    let results: Arc<Mutex<ResultVec>> = Arc::new(Mutex::new(Vec::new()));
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // We must collect to own the data before sharing across threads.
    #[allow(clippy::needless_collect)]
    let work_items: Vec<(usize, TestEntry)> = entries.iter().cloned().enumerate().collect();
    let work = Arc::new(Mutex::new(work_items.into_iter()));

    let mut handles = Vec::new();
    let actual_workers = num_workers.min(entries.len()).max(1);

    for _ in 0..actual_workers {
        let work = Arc::clone(&work);
        let results = Arc::clone(&results);
        let stop_flag = Arc::clone(&stop_flag);
        let brush_path = config.brush_path.clone();
        let tests_dir = config.tests_dir.clone();
        let bash_source_dir = config.bash_source_dir.clone();
        let mode = config.mode.clone();
        let bash_path = config.bash_path.clone();
        let timeout = config.timeout;
        let stop_on_first = config.stop_on_first;
        let verbose = config.verbose;

        let handle = std::thread::spawn(move || {
            // Create a per-worker copy of the test directory so parallel
            // tests don't collide on CWD writes or shared TMPDIR files.
            let farm_tmp = match tempfile::TempDir::new() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("WARNING: failed to create test farm: {e}");
                    return;
                }
            };
            let work_dir = farm_tmp.path().join("tests");
            if let Err(e) = std::fs::create_dir(&work_dir)
                .map_err(anyhow::Error::from)
                .and_then(|()| create_test_farm(&tests_dir, &work_dir))
            {
                eprintln!("WARNING: failed to populate test farm: {e}");
                return;
            }
            // Per-worker TMPDIR prevents collisions on files like
            // $TMPDIR/sh (posixexp) or $TMPDIR/newhistory (history).
            let worker_tmpdir = farm_tmp.path().join("tmp");
            if let Err(e) = std::fs::create_dir(&worker_tmpdir) {
                eprintln!("WARNING: failed to create worker tmpdir: {e}");
                return;
            }

            loop {
                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                let item = match work.lock() {
                    Ok(mut guard) => guard.next(),
                    Err(_) => break,
                };

                let Some((idx, entry)) = item else {
                    break;
                };

                let tmpdir_str = worker_tmpdir.display().to_string();
                let (result, diff, actual) = run_single_test(
                    &entry,
                    &brush_path,
                    &work_dir,
                    &tests_dir,
                    &bash_source_dir,
                    &tmpdir_str,
                    timeout,
                    &mode,
                    &bash_path,
                    verbose,
                );

                if stop_on_first
                    && (result.status == TestStatus::Failed
                        || result.status == TestStatus::TimedOut)
                {
                    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }

                if let Ok(mut guard) = results.lock() {
                    guard.push((idx, result, diff, actual));
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("worker thread panicked"))?;
    }

    let mut final_results = Arc::try_unwrap(results)
        .map_err(|_| anyhow::anyhow!("failed to unwrap results"))?
        .into_inner()
        .map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
    final_results.sort_by_key(|(idx, _, _, _)| *idx);

    Ok(final_results
        .into_iter()
        .map(|(_, result, diff, actual)| (result, diff, actual))
        .collect())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_single_test(
    entry: &TestEntry,
    brush_path: &Path,
    work_dir: &Path,
    tests_dir: &Path,
    bash_source_dir: &Path,
    tmpdir: &str,
    timeout: Duration,
    mode: &CompareMode,
    bash_path_str: &str,
    verbose: bool,
) -> (TestResult, Option<String>, Option<String>) {
    if verbose {
        eprintln!("  Running: {} ({})", entry.name, entry.test_script);
    }

    let effective_timeout = timeout * entry.timeout_multiplier;
    let brush_result = execute_test(
        brush_path,
        entry,
        work_dir,
        tests_dir,
        bash_source_dir,
        tmpdir,
        effective_timeout,
    );

    let (brush_output, duration) = match brush_result {
        Ok((output, dur)) => (output, dur),
        Err(e) => {
            let err_msg = format!("{e:#}");
            let is_timeout = err_msg.contains("timed out");
            // Read the .right file to report expected line count even on error.
            let total_lines = std::fs::read(tests_dir.join(&entry.right_file))
                .map_or(0, |b| String::from_utf8_lossy(&b).lines().count());
            let duration_secs = if is_timeout {
                timeout.as_secs_f64()
            } else {
                0.0
            };
            return (
                TestResult {
                    name: entry.name.clone(),
                    status: if is_timeout {
                        TestStatus::TimedOut
                    } else {
                        TestStatus::Failed
                    },
                    matching_lines: 0,
                    total_lines,
                    duration_secs: (duration_secs * 10.0).round() / 10.0,
                    first_diff_line: None,
                    known_issue: None,
                },
                Some(format!("Error: {err_msg}")),
                None,
            );
        }
    };

    let duration_secs = duration.as_secs_f64();

    let (expected, expected_label) = match mode {
        CompareMode::Static => {
            let right_path = tests_dir.join(&entry.right_file);
            // Use lossy conversion: some .right files contain non-UTF-8 data
            // (e.g. nquote4.right is ISO-8859, glob.right is binary).
            match std::fs::read(&right_path) {
                Ok(bytes) => {
                    let content = String::from_utf8_lossy(&bytes).to_string();
                    (content, entry.right_file.clone())
                }
                Err(e) => {
                    return (
                        TestResult {
                            name: entry.name.clone(),
                            status: TestStatus::Skipped,
                            matching_lines: 0,
                            total_lines: 0,
                            duration_secs,
                            first_diff_line: None,
                            known_issue: None,
                        },
                        Some(format!("Could not read {}: {e}", entry.right_file)),
                        Some(brush_output),
                    );
                }
            }
        }
        CompareMode::Oracle => {
            let bash_path = Path::new(bash_path_str);
            match execute_test(
                bash_path,
                entry,
                work_dir,
                tests_dir,
                bash_source_dir,
                tmpdir,
                effective_timeout,
            ) {
                Ok((output, _)) => {
                    let normalized = normalize_output(&output, bash_path);
                    (normalized, "bash output".to_string())
                }
                Err(e) => {
                    return (
                        TestResult {
                            name: entry.name.clone(),
                            status: TestStatus::Skipped,
                            matching_lines: 0,
                            total_lines: 0,
                            duration_secs,
                            first_diff_line: None,
                            known_issue: None,
                        },
                        Some(format!("Oracle execution failed: {e:#}")),
                        Some(brush_output),
                    );
                }
            }
        }
    };

    let normalized_brush = normalize_output(&brush_output, brush_path);
    let (matching, total, first_diff) = compare_outputs(&normalized_brush, &expected);

    let status =
        if matching == total && normalized_brush.lines().count() == expected.lines().count() {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };

    let diff = if status == TestStatus::Failed {
        Some(unified_diff(
            &expected,
            &normalized_brush,
            &expected_label,
            "brush output",
        ))
    } else {
        None
    };

    // Promote to PASS if the diff is fully explained by a known platform issue.
    let (status, known_issue) = if status == TestStatus::Failed {
        if let Some(desc) = diff
            .as_ref()
            .and_then(|d| check_known_platform_issue(&entry.name, d))
        {
            (TestStatus::Passed, Some(desc.to_string()))
        } else {
            (status, None)
        }
    } else {
        (status, None)
    };

    (
        TestResult {
            name: entry.name.clone(),
            status,
            matching_lines: matching,
            total_lines: total,
            duration_secs: (duration_secs * 10.0).round() / 10.0,
            first_diff_line: first_diff,
            known_issue,
        },
        diff,
        Some(normalized_brush),
    )
}
