//! Containerized end-to-end tests that run real applications' own shell-integration
//! suites against the shell under test.
//!
//! Each adapter lives in `e2e/<app>/`: a Dockerfile whose entrypoint runs the suite and
//! writes `log.txt` plus `JUnit` XML under `/results`. This module builds and runs those
//! images, then decides each adapter's result from the `JUnit` it left behind, weighed
//! against the adapter's `xfail-list.txt`. See `e2e/README.md` for the full contract.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::{IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Args;
use clap::builder::styling::{AnsiColor, Style};

use crate::common::find_workspace_root;
use crate::test::BinaryArgs;

/// Arguments for containerized end-to-end tests.
#[derive(Args, Clone)]
pub struct E2eArgs {
    /// Applications to test. Runs all adapters except blesh when omitted.
    #[clap(value_name = "APP")]
    apps: Vec<String>,

    /// Test the applications against Bash instead of brush.
    #[clap(long, conflicts_with = "shell")]
    baseline: bool,

    /// Test the applications against another shell binary.
    #[clap(long, value_name = "PATH", conflicts_with = "baseline")]
    shell: Option<PathBuf>,

    /// Seconds an adapter's container may run before it is killed. Bounds a hung shell: only
    /// ble.sh bounds itself, and its suite alone takes about nine minutes.
    #[clap(long, value_name = "SECS", default_value_t = 1800)]
    timeout: u64,

    /// Root directory for retained run directories containing per-application results.
    #[clap(long, value_name = "DIR", default_value = "target/e2e")]
    results_dir: PathBuf,

    /// Arguments passed to a single selected adapter, after `--`.
    #[clap(last = true, value_name = "ARG")]
    adapter_args: Vec<OsString>,
}

pub fn run(binary_args: &BinaryArgs, args: &E2eArgs, verbose: bool) -> Result<()> {
    let workspace = find_workspace_root()?;
    let e2e_dir = workspace.join("e2e");
    let apps = select_e2e_apps(&e2e_dir, args)?;
    let shell = resolve_shell(binary_args, args)?;
    // Read up front: an unreadable list should fail before any container time is spent, not
    // halfway through a run whose earlier reports would then be lost. `--baseline`/`--shell`
    // runs carry no expectations, since those describe brush.
    let expectations: Vec<BTreeSet<String>> = if args.baseline || args.shell.is_some() {
        vec![BTreeSet::new(); apps.len()]
    } else {
        apps.iter()
            .map(|app| read_test_list(&e2e_dir.join(app).join("xfail-list.txt")))
            .collect::<Result<_>>()?
    };

    let results_root = if args.results_dir.is_absolute() {
        args.results_dir.clone()
    } else {
        workspace.join(&args.results_dir)
    };
    let docker = std::env::var_os("DOCKER").unwrap_or_else(|| "docker".into());
    let results_root = create_e2e_results_root(&results_root)?;

    let shell_name = shell.as_deref().map_or_else(
        || "bash".to_owned(),
        |path| {
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned()
        },
    );
    eprintln!(
        "    Starting {} e2e adapter(s) against {shell_name}",
        apps.len()
    );
    eprintln!(
        "     Results {}/<app>/ (log.txt, junit/*.xml)",
        results_root.display()
    );

    let overall = Instant::now();
    let terminal = std::io::stderr().is_terminal();
    let color = terminal
        && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && std::env::var_os("TERM").is_none_or(|value| value != "dumb");
    // On a terminal, name the adapter while it runs: blesh alone takes about nine minutes, and a
    // silent wait that long reads as a hang. Piped output skips the marker so logs stay clean, as
    // does `--verbose`, whose streamed output would scroll over it anyway.
    let progress = terminal && !verbose;
    let run = E2eRun {
        docker: &docker,
        e2e_dir: &e2e_dir,
        results_root: &results_root,
        shell: shell.as_deref(),
        adapter_args: &args.adapter_args,
        timeout: Duration::from_secs(args.timeout),
        verbose,
    };
    let mut reports = Vec::new();
    for (app, expected) in apps.iter().zip(&expectations) {
        if progress {
            let style = e2e_style(color, AnsiColor::Cyan);
            eprint!("        {style}run {style:#} [        ] {app}\r");
            let _ = std::io::stderr().flush();
        }
        let started = Instant::now();
        let outcome = run_e2e_adapter(&run, app);
        let elapsed = started.elapsed();
        let results = results_root.join(app);
        let report = AdapterReport::new(
            app.clone(),
            // Shown to the user, so prefer the short workspace-relative form.
            results
                .strip_prefix(&workspace)
                .unwrap_or(&results)
                .to_path_buf(),
            elapsed,
            outcome,
            read_junit_summary(&results, expected, args.adapter_args.is_empty()),
        );
        eprintln!("{}", report.line(color));
        reports.push(report);
    }

    report_e2e_results(&reports, overall.elapsed(), color)
}

/// The adapters to run, rejecting a selection this runner cannot honor.
fn select_e2e_apps(e2e_dir: &Path, args: &E2eArgs) -> Result<Vec<String>> {
    let available = discover_e2e_apps(e2e_dir, !args.apps.is_empty())?;
    let apps = if args.apps.is_empty() {
        available
    } else {
        for app in &args.apps {
            if !available.contains(app) {
                anyhow::bail!(
                    "unknown e2e adapter '{app}'; available adapters: {}",
                    available.join(", ")
                );
            }
        }
        args.apps.clone()
    };

    if !args.adapter_args.is_empty() && apps.len() != 1 {
        anyhow::bail!("adapter arguments require exactly one selected application");
    }
    Ok(apps)
}

/// The shell binary to bind-mount into each container, or `None` for the container's own bash.
fn resolve_shell(binary_args: &BinaryArgs, args: &E2eArgs) -> Result<Option<PathBuf>> {
    if (args.baseline || args.shell.is_some()) && binary_args.brush_path.is_some() {
        anyhow::bail!("--brush-path cannot be combined with --baseline or --shell");
    }
    if args.baseline {
        return Ok(None);
    }
    let Some(path) = &args.shell else {
        return Ok(Some(binary_args.find_brush_binary()?));
    };
    // Resolved, since a symlink or a relative path would not survive the bind mount.
    let path = path
        .canonicalize()
        .with_context(|| format!("shell not found at: {}", path.display()))?;
    anyhow::ensure!(path.is_file(), "shell is not a file: {}", path.display());
    Ok(Some(path))
}

/// Retain a fresh directory for each invocation; never clear caller-provided directories.
fn create_e2e_results_root(root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create results root: {}", root.display()))?;
    Ok(tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(root)
        .context("failed to create e2e run directory")?
        .keep())
}

fn e2e_style(color: bool, foreground: AnsiColor) -> Style {
    if color {
        Style::new().bold().fg_color(Some(foreground.into()))
    } else {
        Style::new()
    }
}

/// Prints the closing summary, then the failing test names and where to read the rest.
fn report_e2e_results(reports: &[AdapterReport], elapsed: Duration, color: bool) -> Result<()> {
    eprintln!("{}", "-".repeat(12));
    let failed: Vec<&AdapterReport> = reports.iter().filter(|report| report.failed).collect();
    let style = e2e_style(
        color,
        if failed.is_empty() {
            AnsiColor::Green
        } else {
            AnsiColor::Red
        },
    );
    eprintln!(
        "     {style}Summary{style:#} [{:>8}] {} adapter(s): {} passed, {} failed",
        format_duration(elapsed),
        reports.len(),
        reports.len() - failed.len(),
        failed.len()
    );

    let noted: Vec<(&AdapterReport, Vec<String>)> = reports
        .iter()
        .map(|report| (report, report.detail()))
        .filter(|(_, detail)| !detail.is_empty())
        .collect();
    if !noted.is_empty() {
        eprintln!();
    }
    for (report, detail) in &noted {
        for (index, line) in detail.iter().enumerate() {
            let app = if index == 0 { report.app.as_str() } else { "" };
            eprintln!("  {app:<9} {line}");
        }
        eprintln!(
            "  {:<9} -> {}",
            "",
            report.results.join("log.txt").display()
        );
    }
    if failed.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "{} e2e adapter(s) failed: {}",
        failed.len(),
        failed
            .iter()
            .map(|report| report.app.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// What one adapter did, for the run report.
struct AdapterReport {
    app: String,
    failed: bool,
    /// Why the run itself went wrong: the adapter could not be run (image build failed, docker
    /// missing), its container exited non-zero, or its report could not be read.
    error: Option<String>,
    elapsed: Duration,
    summary: Option<JunitSummary>,
    results: PathBuf,
}

impl AdapterReport {
    fn new(
        app: String,
        results: PathBuf,
        elapsed: Duration,
        outcome: Result<Option<String>>,
        summary: Result<JunitSummary>,
    ) -> Self {
        // A build that failed, or a docker that would not start, means the adapter never ran, so
        // nothing in its JUnit can excuse it. Only a container that did run gets that benefit,
        // since every adapter exits non-zero when a test expected to fail does.
        let (mut error, fatal) = match outcome {
            Ok(exit_error) => (exit_error, false),
            Err(error) => (Some(format!("{error:#}")), true),
        };
        let summary = match summary {
            Ok(summary) => Some(summary),
            Err(report_error) => {
                error = Some(match error {
                    // When the adapter never ran, its missing report is a consequence, not news.
                    Some(error) if fatal => error,
                    Some(error) => format!("{error}; {report_error:#}"),
                    None => format!("{report_error:#}"),
                });
                None
            }
        };
        // A missing or invalid report cannot establish a passing run either, even when the
        // container exits successfully.
        let failed = match &summary {
            Some(summary) => {
                fatal || summary.is_failure() || error.is_some() && !summary.has_expectations()
            }
            None => true,
        };
        Self {
            app,
            failed,
            error,
            elapsed,
            summary,
            results,
        }
    }

    fn line(&self, color: bool) -> String {
        let status = if self.failed { "FAIL" } else { "PASS" };
        let style = e2e_style(
            color,
            if self.failed {
                AnsiColor::Red
            } else if self
                .summary
                .as_ref()
                .is_some_and(JunitSummary::has_expectations)
            {
                AnsiColor::Yellow
            } else {
                AnsiColor::Green
            },
        );
        let counts = self
            .summary
            .as_ref()
            .map_or_else(|| "no results".to_owned(), JunitSummary::counts);
        format!(
            "        {style}{status}{style:#} [{:>8}] {:<9} {counts}",
            format_duration(self.elapsed),
            self.app
        )
    }

    /// What is worth reading about this adapter: the offending test names grouped by what went
    /// wrong, the tests that did not run, or the run error when there are no results at all.
    /// Empty when a clean pass leaves nothing to say.
    fn detail(&self) -> Vec<String> {
        let Some(summary) = &self.summary else {
            return vec![
                self.error
                    .clone()
                    .unwrap_or_else(|| "failed with no results".to_owned()),
            ];
        };
        let mut lines = Vec::new();
        for (names, label) in [
            (&summary.failures, "failed"),
            // Both of these mean the xfail list no longer matches reality, so say what to do.
            (
                &summary.unexpected_passes,
                "passed unexpectedly; remove from xfail-list.txt",
            ),
            (
                &summary.stale_expectations,
                "in xfail-list.txt but did not run; remove or fix the name",
            ),
        ] {
            if !names.is_empty() {
                lines.push(format!("{label}: {}", summarize_names(names)));
            }
        }
        if !summary.skips.is_empty() {
            lines.push(format!("did not run: {}", summarize_names(&summary.skips)));
        }
        if self.failed && lines.is_empty() {
            lines.push(
                self.error
                    .clone()
                    .unwrap_or_else(|| "failed with no reported test failures".to_owned()),
            );
        }
        lines
    }
}

/// A few names, with a count for the rest.
fn summarize_names(names: &[String]) -> String {
    use std::fmt::Write;
    let shown = names.len().min(3);
    let mut text = names[..shown].join(", ");
    if names.len() > shown {
        let _ = write!(text, " (+{} more)", names.len() - shown);
    }
    text
}

/// Seconds for short runs, minutes and seconds once a suite runs long (blesh takes ~9 minutes).
fn format_duration(elapsed: Duration) -> String {
    // Round to tenths first, in integers: rounding the seconds and the remainder separately
    // would print 119.97s as `1m60.0s`, and integers need no lossy cast.
    let tenths = (elapsed.as_millis() + 50) / 100;
    let (secs, tenth) = (tenths / 10, tenths % 10);
    if secs >= 60 {
        format!("{}m{:02}.{tenth}s", secs / 60, secs % 60)
    } else {
        format!("{secs}.{tenth}s")
    }
}

/// Totals read back from the `JUnit` an adapter's runner wrote.
///
/// Every adapter emits `JUnit` -- bats, pytest, minitest-ci and the two hand-rolled reporters all
/// do -- so this is the one shape that summarizes all of them.
#[derive(Default)]
struct JunitSummary {
    tests: usize,
    /// Names of the cases the runner did not run. Most come from `skip-list.txt`, but a suite can
    /// also skip a test itself, which nothing else would report.
    skips: Vec<String>,
    /// Known gaps that still run, so an unexpected fix is reported. pytest writes these as
    /// `<skipped type="pytest.xfail">`; counting them apart from plain skips keeps a suite that
    /// is entirely expected-failure (blesh, today) from reading as if it passed outright.
    xfailed: usize,
    /// The subset of `xfailed` that `xfail-list.txt` predicted. Only these make the suite exit
    /// non-zero: a runner that decides an expected failure itself still reports the run as green.
    listed_failures: usize,
    /// Names of the failing cases that were not expected to fail.
    failures: Vec<String>,
    /// Cases listed in `xfail-list.txt` that passed anyway: the gap they track is closed, and the
    /// entry has to go. Reported as a failure so a fix cannot land unnoticed.
    unexpected_passes: Vec<String>,
    /// Entries in `xfail-list.txt` matching no test that actually ran, so the list has gone
    /// stale: the test was renamed, removed, or moved to `skip-list.txt`.
    stale_expectations: Vec<String>,
}

impl JunitSummary {
    /// A count breakdown listing only the buckets with entries.
    fn counts(&self) -> String {
        let mut parts = vec![format!("{} tests", self.tests)];
        for (n, label) in [
            (self.failures.len(), "failed"),
            (self.unexpected_passes.len(), "unexpectedly passed"),
            (
                self.stale_expectations.len(),
                if self.stale_expectations.len() == 1 {
                    "stale xfail entry"
                } else {
                    "stale xfail entries"
                },
            ),
            (self.xfailed, "xfailed"),
            (self.skips.len(), "skipped"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {label}"));
            }
        }
        parts.join(", ")
    }

    /// Folds in one test case, consulting `expected` (the adapter's `xfail-list.txt`).
    ///
    /// An entry matches either the bare test name or the `classname::name` form, since what a
    /// runner puts in each field differs.
    fn record(&mut self, case: junit_parser::TestCase, expected: &BTreeSet<String>) {
        self.tests += 1;
        let expected_to_fail =
            expected.contains(&case.original_name) || expected.contains(&case.name);
        match case.status {
            junit_parser::TestStatus::Failure(_) | junit_parser::TestStatus::Error(_) => {
                if expected_to_fail {
                    self.xfailed += 1;
                    self.listed_failures += 1;
                } else {
                    // The bare name; `case.name` is prefixed with the class name.
                    self.failures.push(case.original_name);
                }
            }
            junit_parser::TestStatus::Success => {
                if expected_to_fail {
                    self.unexpected_passes.push(case.original_name);
                }
            }
            _ if is_declared_xfail(&case.status) => self.xfailed += 1,
            junit_parser::TestStatus::Skipped(_) => self.skips.push(case.original_name),
        }
    }

    /// Whether a test this adapter's `xfail-list.txt` predicted did fail, which is why the suite
    /// may have exited non-zero without that being a problem.
    const fn has_expectations(&self) -> bool {
        self.listed_failures > 0
    }

    /// Whether this run should be reported as a failure.
    const fn is_failure(&self) -> bool {
        !self.failures.is_empty()
            || !self.unexpected_passes.is_empty()
            || !self.stale_expectations.is_empty()
    }
}

/// A `<skipped>` a runner wrote to report an expected failure of its own; pytest's is
/// `<skipped type="pytest.xfail">`. Unlike a plain skip, the test did run.
fn is_declared_xfail(status: &junit_parser::TestStatus) -> bool {
    matches!(status, junit_parser::TestStatus::Skipped(skipped)
        if skipped.skipped_type.starts_with("pytest.xfail"))
}

/// Folds one suite and any suites nested inside it into `summary`, noting in `seen` every test
/// that actually ran.
fn record_suite(
    suite: junit_parser::TestSuite,
    expected: &BTreeSet<String>,
    summary: &mut JunitSummary,
    seen: &mut BTreeSet<String>,
) {
    for case in suite.cases {
        // A plain skip did not run, so it cannot satisfy an expectation: leaving it out of
        // `seen` is what reports an `xfail-list.txt` entry that has moved to `skip-list.txt`.
        if !matches!(case.status, junit_parser::TestStatus::Skipped(_))
            || is_declared_xfail(&case.status)
        {
            seen.insert(case.original_name.clone());
            seen.insert(case.name.clone());
        }
        summary.record(case, expected);
    }
    for nested in suite.suites {
        record_suite(nested, expected, summary, seen);
    }
}

/// Reads one of the adapter's `*-list.txt` files: one entry per line, `#` starts a comment.
fn read_test_list(path: &Path) -> Result<BTreeSet<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(ToOwned::to_owned)
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// Reads and totals every `JUnit` file the adapter left under `junit/`.
///
/// Missing, unreadable, or empty reports are errors: they cannot establish a passing run.
fn read_junit_summary(
    results: &Path,
    expected: &BTreeSet<String>,
    whole_suite: bool,
) -> Result<JunitSummary> {
    let junit = results.join("junit");
    let mut files = Vec::new();
    for entry in fs::read_dir(&junit)
        .with_context(|| format!("failed to read JUnit directory: {}", junit.display()))?
    {
        let path = entry
            .context("failed to read JUnit directory entry")?
            .path();
        if path.extension().is_some_and(|ext| ext == "xml") {
            files.push(path);
        }
    }
    anyhow::ensure!(!files.is_empty(), "no JUnit reports in {}", junit.display());
    files.sort();

    let mut summary = JunitSummary::default();
    let mut seen = BTreeSet::new();
    for file in &files {
        let reader = std::io::BufReader::new(
            fs::File::open(file).with_context(|| format!("failed to open {}", file.display()))?,
        );
        let report = junit_parser::from_reader(reader)
            .with_context(|| format!("failed to parse {}", file.display()))?;
        let before = summary.tests;
        for suite in report.suites {
            record_suite(suite, expected, &mut summary, &mut seen);
        }
        // Adapters write one report per phase, so an empty one means that phase collapsed before
        // it ran anything -- which the other phases' expected failures must not paper over. A
        // subset run is exempt: there, selecting nothing for a phase is the point.
        anyhow::ensure!(
            !whole_suite || summary.tests > before,
            "JUnit report has no test cases: {}",
            file.display()
        );
    }
    // An entry naming nothing that ran means the test was renamed, removed, or is being skipped
    // elsewhere -- either way the list no longer describes reality. Only a run of the whole suite
    // can conclude that, though: when the caller selected a subset, most entries name a test that
    // was simply not part of this run.
    if whole_suite {
        summary.stale_expectations = expected.difference(&seen).cloned().collect();
    }
    anyhow::ensure!(summary.tests > 0, "JUnit reports contain no test cases");
    Ok(summary)
}

fn discover_e2e_apps(e2e_dir: &Path, include_opt_in: bool) -> Result<Vec<String>> {
    let mut apps = Vec::new();
    for entry in fs::read_dir(e2e_dir).context("failed to read e2e adapter directory")? {
        let entry = entry?;
        // ble.sh currently spends minutes on compatibility failures and timeouts; run it explicitly.
        if !include_opt_in && entry.file_name() == "blesh" {
            continue;
        }
        if entry.path().join("Dockerfile").is_file() {
            apps.push(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("e2e adapter name is not valid UTF-8"))?,
            );
        }
    }
    apps.sort();
    anyhow::ensure!(!apps.is_empty(), "no e2e adapters found");
    Ok(apps)
}

/// Everything an adapter run needs that does not vary between adapters.
struct E2eRun<'a> {
    docker: &'a std::ffi::OsStr,
    e2e_dir: &'a Path,
    results_root: &'a Path,
    shell: Option<&'a Path>,
    adapter_args: &'a [OsString],
    timeout: Duration,
    verbose: bool,
}

fn run_e2e_adapter(run: &E2eRun<'_>, app: &str) -> Result<Option<String>> {
    let E2eRun {
        docker,
        e2e_dir,
        results_root,
        shell,
        adapter_args,
        timeout,
        verbose,
    } = *run;
    let image = format!("brush-e2e-{app}");
    let dockerfile = e2e_dir.join(app).join("Dockerfile");
    let results = results_root.join(app);
    fs::create_dir(&results)
        .with_context(|| format!("failed to create results directory: {}", results.display()))?;
    let results = results
        .canonicalize()
        .with_context(|| format!("failed to resolve results directory: {}", results.display()))?;

    let mut build = ProcessCommand::new(docker);
    build.arg("build");
    // A cold build takes minutes; `-q` keeps that noise out of the report, and `--verbose`
    // opts back into the progress output.
    if !verbose {
        build.arg("-q");
    }
    build
        .arg("-t")
        .arg(&image)
        .arg("-f")
        .arg(&dockerfile)
        .arg(e2e_dir);
    let build = run_process(&mut build, timeout, verbose).context("container build failed")?;
    anyhow::ensure!(
        !build.timed_out,
        "container build timed out after {}",
        format_duration(timeout)
    );
    anyhow::ensure!(build.ok, "container build failed{}", tail(&build.output));

    let mut results_mount = results.as_os_str().to_owned();
    results_mount.push(":/results");
    // SAFETY: geteuid takes no arguments and has no preconditions.
    let uid = unsafe { libc::geteuid() };
    // SAFETY: getegid takes no arguments and has no preconditions.
    let gid = unsafe { libc::getegid() };
    let container = format!("{image}-{}", std::process::id());
    let mut test = ProcessCommand::new(docker);
    test.args(["run", "--rm", "--name", &container, "--user"])
        .arg(format!("{uid}:{gid}"))
        .args(["-e", "HOME=/tmp", "-v"])
        .arg(results_mount);
    if let Some(path) = shell {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("shell filename is not valid UTF-8")?;
        let target = format!("/shell/{filename}");
        let mut shell_mount = path.as_os_str().to_owned();
        shell_mount.push(format!(":{target}:ro"));
        test.arg("-v")
            .arg(shell_mount)
            .arg("-e")
            .arg(format!("SHELL_UNDER_TEST={target}"));
    }
    test.arg(&image).args(adapter_args);
    let test = run_process(&mut test, timeout, verbose).context("container test failed")?;
    if test.timed_out {
        // The client is already gone; stop the container it left behind.
        let _ = ProcessCommand::new(docker)
            .args(["kill", &container])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        anyhow::bail!("container timed out after {}", format_duration(timeout));
    }
    // A suite whose only failures are expected still exits non-zero, so this is reported rather
    // than fatal: the caller weighs it against the JUnit, and shows the tail only when no test
    // failure explains it.
    Ok(if test.ok {
        None
    } else {
        Some(format!("container exited non-zero{}", tail(&test.output)))
    })
}

/// The tail of captured output, indented, for the errors that leave no `JUnit` to report from.
fn tail(output: &str) -> String {
    let mut lines: Vec<&str> = output.lines().collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let shown = lines.len().saturating_sub(12);
    let mut text = String::new();
    for line in &lines[shown..] {
        text.push_str("\n            ");
        text.push_str(line);
    }
    text
}

/// Outcome of a docker invocation, with its output when it was captured rather than streamed.
struct ProcessRun {
    ok: bool,
    timed_out: bool,
    output: String,
}

/// Streams the command's output under `--verbose`, and captures it otherwise, giving up on it
/// after `timeout`.
///
/// Quiet is the default because every adapter already tees its full output to
/// `<results>/log.txt`; streaming it as well buries the run report in thousands of lines.
///
/// Output is captured to a temporary file rather than to a pipe: a pipe would have to be drained
/// while waiting, and there is nothing to read it with while this thread watches the clock.
fn run_process(
    command: &mut ProcessCommand,
    timeout: Duration,
    verbose: bool,
) -> Result<ProcessRun> {
    let program = command.get_program().to_string_lossy().into_owned();
    let start = move || format!("failed to start {program}");
    let captured = if verbose {
        eprintln!("Running: {command:?}");
        None
    } else {
        let file = tempfile::tempfile().context("failed to open capture file")?;
        command
            .stdout(file.try_clone().context("failed to open capture file")?)
            .stderr(file.try_clone().context("failed to open capture file")?);
        Some(file)
    };

    let mut child = command.spawn().with_context(start)?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().context("failed to wait for child")? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            // The child here is the docker client; stopping what it started is the caller's job.
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    let output = match captured {
        None => String::new(),
        Some(mut file) => {
            use std::io::{Read as _, Seek as _};
            file.rewind().context("failed to read captured output")?;
            // Lossy: a suite's output is whatever bytes its tests happened to print.
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .context("failed to read captured output")?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };
    Ok(ProcessRun {
        ok: status.is_some_and(|status| status.success()),
        timed_out: status.is_none(),
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_adapter_directories() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::create_dir_all(temp.path().join("fzf"))?;
        fs::write(temp.path().join("fzf/Dockerfile"), "FROM scratch\n")?;
        fs::create_dir_all(temp.path().join("blesh"))?;
        fs::write(temp.path().join("blesh/Dockerfile"), "FROM scratch\n")?;
        fs::create_dir_all(temp.path().join("lib"))?;

        anyhow::ensure!(discover_e2e_apps(temp.path(), false)? == ["fzf"]);
        anyhow::ensure!(discover_e2e_apps(temp.path(), true)? == ["blesh", "fzf"]);
        Ok(())
    }

    /// One case of each shape the adapters' runners actually emit: a self-closing pass
    /// (minitest-ci), a pass with an end tag, a failure, a bats skip, and a pytest xfail --
    /// which is a `<skipped>` and must not be counted as an ordinary skip.
    #[test]
    fn summarizes_junit_from_every_runner_shape() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let junit = temp.path().join("junit");
        fs::create_dir_all(&junit)?;
        fs::write(
            junit.join("a.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <testsuites><testsuite name="s">
              <testcase classname="c" name="self closing pass"/>
              <testcase classname="c" name="pass"></testcase>
              <testcase classname="c" name="broken"><failure message="m">body</failure></testcase>
              <testcase classname="c" name="left out"><skipped/></testcase>
              <testcase classname="c" name="known gap">
                <skipped type="pytest.xfail" message="issue #1"/>
              </testcase>
            </testsuite></testsuites>"#,
        )?;
        // A second file must add to the totals, the way nvm writes upstream and interactive JUnit.
        fs::write(
            junit.join("b.xml"),
            r#"<testsuite name="s2"><testcase name="also broken"><error/></testcase></testsuite>"#,
        )?;

        let summary = read_junit_summary(temp.path(), &BTreeSet::new(), true)?;
        anyhow::ensure!(summary.tests == 6, "tests: {}", summary.tests);
        anyhow::ensure!(summary.skips == ["left out"], "{}", summary.counts());
        anyhow::ensure!(summary.xfailed == 1, "xfailed: {}", summary.xfailed);
        anyhow::ensure!(summary.failures == ["broken", "also broken"]);
        anyhow::ensure!(
            summary.counts() == "6 tests, 2 failed, 1 xfailed, 1 skipped",
            "{}",
            summary.counts()
        );
        Ok(())
    }

    /// Neither missing results nor malformed or empty XML can establish a passing run.
    #[test]
    fn rejects_missing_malformed_and_empty_reports() -> Result<()> {
        let temp = tempfile::tempdir()?;
        anyhow::ensure!(read_junit_summary(temp.path(), &BTreeSet::new(), true).is_err());
        fs::create_dir_all(temp.path().join("junit"))?;
        anyhow::ensure!(read_junit_summary(temp.path(), &BTreeSet::new(), true).is_err());
        let report = temp.path().join("junit/broken.xml");
        fs::write(&report, "<testsuite><testcase></testsuite>")?;
        let error = read_junit_summary(temp.path(), &BTreeSet::new(), true)
            .err()
            .context("malformed XML must fail")?;
        anyhow::ensure!(error.to_string().contains("failed to parse"));
        let report_error = AdapterReport::new(
            "example".to_owned(),
            temp.path().to_path_buf(),
            Duration::ZERO,
            Ok(None),
            Err(error),
        );
        anyhow::ensure!(report_error.failed);
        anyhow::ensure!(report_error.detail()[0].contains("failed to parse"));
        fs::write(&report, "<testsuite name=\"empty\"/>")?;
        anyhow::ensure!(read_junit_summary(temp.path(), &BTreeSet::new(), true).is_err());
        Ok(())
    }

    /// Writes a one-suite report with a failing and a passing case, for the expectation tests.
    fn write_report(dir: &Path) -> Result<()> {
        fs::create_dir_all(dir.join("junit"))?;
        fs::write(
            dir.join("junit/r.xml"),
            r#"<testsuite name="s">
              <testcase classname="c" name="known gap"><failure/></testcase>
              <testcase classname="c" name="works"/>
            </testsuite>"#,
        )?;
        Ok(())
    }

    /// A listed failure is expected, so the run is green and the failure is not reported.
    #[test]
    fn expected_failures_do_not_fail_the_run() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["known gap".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, true)?;
        anyhow::ensure!(summary.xfailed == 1);
        anyhow::ensure!(summary.failures.is_empty());
        anyhow::ensure!(!summary.is_failure(), "{}", summary.counts());
        Ok(())
    }

    /// The point of tracking them: a gap that closes has to be noticed, not silently absorbed.
    #[test]
    fn an_expected_failure_that_passes_fails_the_run() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["known gap".to_owned(), "works".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, true)?;
        anyhow::ensure!(summary.unexpected_passes == ["works"]);
        anyhow::ensure!(summary.is_failure());
        Ok(())
    }

    /// An entry that names nothing that ran means the list has rotted.
    #[test]
    fn a_stale_expectation_fails_the_run() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["renamed away".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, true)?;
        anyhow::ensure!(summary.stale_expectations == ["renamed away"]);
        anyhow::ensure!(summary.is_failure());
        Ok(())
    }

    /// ...but not when the caller ran a subset, where an unmatched entry just names a test that
    /// was not selected.
    #[test]
    fn a_subset_run_does_not_report_stale_expectations() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["known gap".to_owned(), "not in this subset".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, false)?;
        anyhow::ensure!(summary.stale_expectations.is_empty());
        anyhow::ensure!(!summary.is_failure(), "{}", summary.counts());
        Ok(())
    }

    /// An entry may name the bare test or the `classname::name` form a runner reports.
    #[test]
    fn expectations_match_either_name_form() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["c::known gap".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, true)?;
        anyhow::ensure!(summary.xfailed == 1, "{}", summary.counts());
        anyhow::ensure!(summary.stale_expectations.is_empty());
        Ok(())
    }

    #[test]
    fn reads_a_list_ignoring_comments_and_blanks() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let list = temp.path().join("xfail-list.txt");
        fs::write(&list, "# why\n\n  a test  \n# another\nsecond\n")?;
        anyhow::ensure!(
            read_test_list(&list)? == BTreeSet::from(["a test".to_owned(), "second".to_owned()])
        );
        // A missing list is simply no expectations.
        anyhow::ensure!(read_test_list(&temp.path().join("nope.txt"))?.is_empty());
        Ok(())
    }

    #[test]
    fn run_directories_preserve_existing_files_and_previous_results() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let adapter = temp.path().join("starship");
        fs::create_dir(&adapter)?;
        fs::write(adapter.join("Dockerfile"), "FROM scratch\n")?;
        let first = create_e2e_results_root(temp.path())?;
        write_report(&first)?;
        let second = create_e2e_results_root(temp.path())?;

        anyhow::ensure!(first != second);
        anyhow::ensure!(fs::read_to_string(adapter.join("Dockerfile"))? == "FROM scratch\n");
        anyhow::ensure!(read_junit_summary(&first, &BTreeSet::new(), true)?.tests == 2);
        anyhow::ensure!(fs::read_dir(second)?.next().is_none());
        Ok(())
    }

    #[test]
    fn report_colors_distinguish_clean_expected_and_failed_outcomes() {
        let mut report = AdapterReport {
            app: "example".to_owned(),
            failed: false,
            error: None,
            elapsed: Duration::from_secs(1),
            summary: Some(JunitSummary::default()),
            results: PathBuf::new(),
        };
        assert!(!report.line(false).contains('\x1b'));
        assert!(report.line(true).contains("\x1b[32mPASS"));
        // Yellow marks a pass that a listed expectation explains.
        report.summary.as_mut().unwrap().listed_failures = 1;
        assert!(report.line(true).contains("\x1b[33mPASS"));
        report.failed = true;
        assert!(report.line(true).contains("\x1b[31mFAIL"));
        assert!(!report.line(false).contains('\x1b'));
    }

    #[test]
    fn formats_durations_for_short_and_long_runs() {
        assert_eq!(format_duration(Duration::from_millis(1200)), "1.2s");
        assert_eq!(format_duration(Duration::from_secs_f64(542.18)), "9m02.2s");
        // Rounding must carry into the minutes rather than printing 60 seconds.
        assert_eq!(format_duration(Duration::from_secs_f64(119.97)), "2m00.0s");
        assert_eq!(format_duration(Duration::from_secs_f64(59.97)), "1m00.0s");
    }

    /// Cases in a nested `<testsuite>` count like any other; dropping them would hide failures.
    #[test]
    fn nested_suites_are_counted() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::create_dir_all(temp.path().join("junit"))?;
        fs::write(
            temp.path().join("junit/n.xml"),
            r#"<testsuites><testsuite name="outer">
                 <testcase classname="c" name="top"/>
                 <testsuite name="inner">
                   <testcase classname="c" name="nested"><failure/></testcase>
                 </testsuite>
               </testsuite></testsuites>"#,
        )?;

        let summary = read_junit_summary(temp.path(), &BTreeSet::new(), true)?;
        anyhow::ensure!(summary.tests == 2, "{}", summary.counts());
        anyhow::ensure!(summary.failures == ["nested"]);
        Ok(())
    }

    /// A test that only skips never runs, so an expectation naming it has gone stale -- otherwise
    /// moving a test to `skip-list.txt` would silence its `xfail-list.txt` entry forever.
    #[test]
    fn an_expectation_for_a_skipped_test_is_stale() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::create_dir_all(temp.path().join("junit"))?;
        fs::write(
            temp.path().join("junit/s.xml"),
            r#"<testsuite name="s">
                 <testcase classname="c" name="not run"><skipped/></testcase>
                 <testcase classname="c" name="self declared">
                   <skipped type="pytest.xfail"/>
                 </testcase>
               </testsuite>"#,
        )?;
        let expected = BTreeSet::from(["not run".to_owned(), "self declared".to_owned()]);

        let summary = read_junit_summary(temp.path(), &expected, true)?;
        // The runner's own xfail did run, so only the plain skip is stale.
        anyhow::ensure!(
            summary.stale_expectations == ["not run"],
            "{}",
            summary.counts()
        );
        Ok(())
    }

    /// A runner that declares its own expected failure still exits zero, so such a case cannot
    /// explain a non-zero exit the way a listed expectation does.
    #[test]
    fn a_runner_declared_xfail_does_not_excuse_a_non_zero_exit() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::create_dir_all(temp.path().join("junit"))?;
        fs::write(
            temp.path().join("junit/x.xml"),
            r#"<testsuite name="s">
                 <testcase classname="c" name="declared"><skipped type="pytest.xfail"/></testcase>
               </testsuite>"#,
        )?;

        let summary = read_junit_summary(temp.path(), &BTreeSet::new(), true)?;
        anyhow::ensure!(summary.xfailed == 1 && !summary.has_expectations());
        anyhow::ensure!(
            AdapterReport::new(
                "example".to_owned(),
                temp.path().to_path_buf(),
                Duration::ZERO,
                Ok(Some("container exited non-zero".to_owned())),
                Ok(summary),
            )
            .failed
        );
        Ok(())
    }

    /// A report file with no cases in it means that phase of the suite never ran.
    #[test]
    fn an_empty_report_file_fails_a_whole_suite_run() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        fs::write(
            temp.path().join("junit/empty.xml"),
            r#"<testsuite name="collapsed" tests="0"/>"#,
        )?;

        anyhow::ensure!(read_junit_summary(temp.path(), &BTreeSet::new(), true).is_err());
        // A subset run may legitimately select nothing for one phase.
        anyhow::ensure!(read_junit_summary(temp.path(), &BTreeSet::new(), false)?.tests == 2);
        Ok(())
    }

    /// Expected failures explain a non-zero exit, but not a failure to run the adapter at all.
    #[test]
    fn only_a_container_that_ran_has_its_exit_excused() -> Result<()> {
        let temp = tempfile::tempdir()?;
        write_report(temp.path())?;
        let expected = BTreeSet::from(["known gap".to_owned()]);
        let report = |outcome| -> Result<bool> {
            Ok(AdapterReport::new(
                "example".to_owned(),
                temp.path().to_path_buf(),
                Duration::ZERO,
                outcome,
                read_junit_summary(temp.path(), &expected, true),
            )
            .failed)
        };

        anyhow::ensure!(!report(Ok(Some("container exited non-zero".to_owned())))?);
        anyhow::ensure!(report(Err(anyhow::anyhow!("container build failed")))?);
        Ok(())
    }
}
