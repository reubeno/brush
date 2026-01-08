//! Test runner implementation.

use crate::comparison::{
    DirComparison, DurationComparison, ExitStatusComparison, ExpectationComparison,
    OracleComparison, SingleExpectationComparison, SnapshotResult, StringComparison,
    TestComparison, diff_dirs, output_matches,
};
use crate::config::{RunnerConfig, TestMode, TestOptions};
use crate::reporting::{TestCaseResult, TestCaseSetResults};
use crate::testcase::{TestCase, TestCaseSet};
use anyhow::{Context, Result};
use colored::Colorize;

/// The main test runner.
pub struct TestRunner {
    config: RunnerConfig,
    options: TestOptions,
}

impl TestRunner {
    /// Creates a new test runner with the given configuration and options.
    pub const fn new(config: RunnerConfig, options: TestOptions) -> Self {
        Self { config, options }
    }

    /// Runs all tests and returns success/failure.
    pub async fn run(&self) -> Result<bool> {
        let mut success_count = 0;
        let mut skip_count = 0;
        let mut known_failure_count = 0;
        let mut fail_count = 0;
        let mut join_handles = vec![];
        let mut success_duration = std::time::Duration::default();

        // Generate a glob pattern to find all the YAML test case files.
        let glob_pattern = self
            .config
            .test_cases_dir
            .join("**/*.yaml")
            .to_string_lossy()
            .to_string();

        if self.options.verbose {
            eprintln!("Running test cases: {glob_pattern}");
        }

        // Spawn each test case set separately.
        for entry in glob::glob(glob_pattern.as_ref()).unwrap() {
            let entry = entry.unwrap();

            let yaml_file = std::fs::File::open(entry.as_path())?;
            let mut test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)
                .context(format!("parsing {}", entry.to_string_lossy()))?;

            test_case_set.source_dir = entry.parent().unwrap().to_path_buf();
            test_case_set.source_file.clone_from(&entry);

            if self.options.list_tests_only {
                for test_case in &test_case_set.cases {
                    let case_is_skipped = self.should_skip_test(&test_case_set, test_case)?;
                    if case_is_skipped == self.options.skipped_tests_only {
                        println!(
                            "{}::{}: test",
                            test_case_set.name.as_deref().unwrap_or("unnamed"),
                            test_case.name.as_deref().unwrap_or("unnamed"),
                        );
                    }
                }
            } else {
                let config = self.config.clone();
                let options = self.options.clone();

                join_handles.push(tokio::spawn(async move {
                    run_test_case_set(test_case_set, config, options).await
                }));
            }
        }

        if self.options.list_tests_only {
            return Ok(true);
        }

        // Await all results.
        let mut all_results = vec![];
        for join_handle in join_handles {
            let results = join_handle.await??;

            success_count += results.success_count;
            skip_count += results.skip_count;
            known_failure_count += results.known_failure_count;
            fail_count += results.fail_count;
            success_duration += results.success_duration;

            all_results.push(results);
        }

        crate::reporting::report_results(all_results, &self.options)?;

        if matches!(self.options.format, crate::config::OutputFormat::Pretty) {
            let formatted_fail_count = if fail_count > 0 {
                fail_count.to_string().red()
            } else {
                fail_count.to_string().green()
            };

            let formatted_known_failure_count = if known_failure_count > 0 {
                known_failure_count.to_string().magenta()
            } else {
                known_failure_count.to_string().green()
            };

            let formatted_skip_count = if skip_count > 0 {
                skip_count.to_string().cyan()
            } else {
                skip_count.to_string().green()
            };

            eprintln!(
                "================================================================================"
            );
            eprintln!(
                "{} test case(s) ran: {} succeeded, {} failed, {} known to fail, {} skipped.",
                success_count + fail_count + known_failure_count,
                success_count.to_string().green(),
                formatted_fail_count,
                formatted_known_failure_count,
                formatted_skip_count,
            );
            eprintln!("duration of successful tests: {success_duration:?}");
            eprintln!(
                "================================================================================"
            );
        }

        Ok(fail_count == 0)
    }

    fn should_skip_test(&self, test_case_set: &TestCaseSet, test_case: &TestCase) -> Result<bool> {
        // Check incompatible configs
        if let Some(oracle) = &self.config.oracle {
            if test_case.incompatible_configs.contains(&oracle.name) {
                return Ok(true);
            }
        }

        // Check incompatible OS
        if let Some(host_os_id) = &self.config.host_os_id {
            if test_case.incompatible_os.contains(host_os_id) {
                return Ok(true);
            }
        }

        // Check oracle version constraints
        if let Some(oracle) = &self.config.oracle {
            if test_case.min_oracle_version.is_some() || test_case.max_oracle_version.is_some() {
                if let Some(actual_oracle_version_str) = &oracle.version_str {
                    let actual_oracle_version =
                        version_compare::Version::from(actual_oracle_version_str.as_str())
                            .ok_or_else(|| anyhow::anyhow!("failed to parse oracle version"))?;

                    if let Some(min_oracle_version_str) = &test_case.min_oracle_version {
                        let min_oracle_version = version_compare::Version::from(
                            min_oracle_version_str,
                        )
                        .ok_or_else(|| anyhow::anyhow!("failed to parse min oracle version"))?;

                        if matches!(
                            actual_oracle_version.compare(min_oracle_version),
                            version_compare::Cmp::Lt
                        ) {
                            return Ok(true);
                        }
                    }

                    if let Some(max_oracle_version_str) = &test_case.max_oracle_version {
                        let max_oracle_version = version_compare::Version::from(
                            max_oracle_version_str,
                        )
                        .ok_or_else(|| anyhow::anyhow!("failed to parse max oracle version"))?;

                        if matches!(
                            actual_oracle_version.compare(max_oracle_version),
                            version_compare::Cmp::Gt
                        ) {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        // Check filters
        let test_case_set_name = test_case_set.name.as_deref().unwrap_or("");
        let test_case_name = test_case.name.as_deref().unwrap_or("");

        if test_case_set_name.is_empty() || test_case_name.is_empty() {
            return Ok(false);
        }

        let qualified_name = format!("{test_case_set_name}::{test_case_name}");
        if !self.options.should_run_test(&qualified_name) {
            return Ok(true);
        }

        Ok(test_case.skip)
    }
}

async fn run_test_case_set(
    test_case_set: TestCaseSet,
    config: RunnerConfig,
    options: TestOptions,
) -> Result<TestCaseSetResults> {
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut known_failure_count = 0;
    let mut fail_count = 0;
    let mut success_duration = std::time::Duration::default();
    let mut test_case_results = vec![];

    // Check if the entire test case set is incompatible
    if let Some(oracle) = &config.oracle {
        if test_case_set.incompatible_configs.contains(&oracle.name) {
            return Ok(TestCaseSetResults {
                name: test_case_set.name.clone(),
                config_name: oracle.name.clone(),
                test_case_results: vec![],
                success_count: 0,
                #[expect(clippy::cast_possible_truncation)]
                skip_count: test_case_set.cases.len() as u32,
                known_failure_count: 0,
                fail_count: 0,
                success_duration: std::time::Duration::default(),
            });
        }
    }

    for test_case in &test_case_set.cases {
        let runner = TestRunner::new(config.clone(), options.clone());
        let case_is_skipped = runner.should_skip_test(&test_case_set, test_case)?;

        let test_case_result = if case_is_skipped == options.skipped_tests_only {
            run_single_test(&test_case_set, test_case, &config).await?
        } else {
            TestCaseResult {
                success: true,
                comparison: TestComparison::skipped(),
                name: test_case.name.clone(),
                skip: true,
                known_failure: test_case.known_failure,
            }
        };

        if test_case_result.skip {
            skip_count += 1;
        } else if test_case_result.success {
            if test_case.known_failure {
                fail_count += 1;
            } else {
                success_count += 1;
                success_duration += test_case_result.comparison.duration;
            }
        } else if test_case.known_failure {
            known_failure_count += 1;
        } else {
            fail_count += 1;
        }

        test_case_results.push(test_case_result);
    }

    let config_name = config
        .oracle
        .map_or_else(|| String::from("brush"), |o| o.name);

    Ok(TestCaseSetResults {
        name: test_case_set.name.clone(),
        config_name,
        test_case_results,
        success_count,
        skip_count,
        known_failure_count,
        fail_count,
        success_duration,
    })
}

async fn run_single_test(
    test_case_set: &TestCaseSet,
    test_case: &TestCase,
    config: &RunnerConfig,
) -> Result<TestCaseResult> {
    let start_time = std::time::Instant::now();

    // Determine what comparisons to perform
    let should_run_oracle = config.oracle.is_some()
        && !test_case.skip_oracle
        && matches!(config.mode, TestMode::Oracle | TestMode::Hybrid);

    let should_check_expectations = test_case.has_expectations()
        || matches!(config.mode, TestMode::Expectation | TestMode::Hybrid);

    // Run oracle comparison if needed
    let (oracle_comparison, test_result, test_temp_dir) = if should_run_oracle {
        let oracle_config = config.oracle.as_ref().unwrap();
        let (oracle_comp, test_res) =
            run_oracle_comparison(test_case_set, test_case, config, oracle_config).await?;
        (Some(oracle_comp), Some(test_res), None)
    } else {
        // Run test shell only
        let test_temp_dir = assert_fs::TempDir::new()?;
        test_case.create_test_files_in(&test_temp_dir, test_case_set)?;
        let test_res = test_case
            .run_shell(&config.test_shell, &test_temp_dir)
            .await?;
        (None, Some(test_res), Some(test_temp_dir))
    };

    // Check expectations
    let expectation_comparison = if should_check_expectations {
        if let Some(test_res) = &test_result {
            check_expectations(
                test_case_set,
                test_case,
                test_res,
                test_temp_dir.as_ref(),
                config,
            )
        } else {
            ExpectationComparison::not_specified()
        }
    } else {
        ExpectationComparison::not_specified()
    };

    let duration = start_time.elapsed();

    let comparison = TestComparison {
        oracle: oracle_comparison,
        expectation: expectation_comparison,
        duration,
    };

    let success = !comparison.is_failure();

    Ok(TestCaseResult {
        success,
        comparison,
        name: test_case.name.clone(),
        skip: false,
        known_failure: test_case.known_failure,
    })
}

async fn run_oracle_comparison(
    test_case_set: &TestCaseSet,
    test_case: &TestCase,
    config: &RunnerConfig,
    oracle_config: &crate::config::OracleConfig,
) -> Result<(OracleComparison, crate::execution::RunResult)> {
    // Run oracle
    let oracle_temp_dir = assert_fs::TempDir::new()?;
    test_case.create_test_files_in(&oracle_temp_dir, test_case_set)?;
    let oracle_result = test_case
        .run_shell(&oracle_config.shell, &oracle_temp_dir)
        .await?;

    // Run test shell
    let test_temp_dir = assert_fs::TempDir::new()?;
    test_case.create_test_files_in(&test_temp_dir, test_case_set)?;
    let test_result = test_case
        .run_shell(&config.test_shell, &test_temp_dir)
        .await?;

    // Build comparison
    let mut comparison = OracleComparison {
        exit_status: ExitStatusComparison::Ignored,
        stdout: StringComparison::Ignored {
            test_string: String::new(),
            oracle_string: String::new(),
        },
        stderr: StringComparison::Ignored {
            test_string: String::new(),
            oracle_string: String::new(),
        },
        temp_dir: DirComparison::Ignored,
        duration: DurationComparison {
            oracle: oracle_result.duration,
            test: test_result.duration,
        },
    };

    // Compare exit status
    if test_case.ignore_exit_status {
        comparison.exit_status = ExitStatusComparison::Ignored;
    } else if oracle_result.exit_status == test_result.exit_status {
        comparison.exit_status = ExitStatusComparison::Same(oracle_result.exit_status);
    } else {
        comparison.exit_status = ExitStatusComparison::TestDiffers {
            test_exit_status: test_result.exit_status,
            oracle_exit_status: oracle_result.exit_status,
        };
    }

    // Compare stdout
    if test_case.ignore_stdout {
        comparison.stdout = StringComparison::Ignored {
            test_string: test_result.stdout.clone(),
            oracle_string: oracle_result.stdout,
        };
    } else if output_matches(
        &oracle_result.stdout,
        &test_result.stdout,
        test_case.ignore_whitespace,
    ) {
        comparison.stdout = StringComparison::Same(oracle_result.stdout);
    } else {
        comparison.stdout = StringComparison::TestDiffers {
            test_string: test_result.stdout.clone(),
            oracle_string: oracle_result.stdout,
        };
    }

    // Compare stderr
    if test_case.ignore_stderr {
        comparison.stderr = StringComparison::Ignored {
            test_string: test_result.stderr.clone(),
            oracle_string: oracle_result.stderr,
        };
    } else if output_matches(
        &oracle_result.stderr,
        &test_result.stderr,
        test_case.ignore_whitespace,
    ) {
        comparison.stderr = StringComparison::Same(oracle_result.stderr);
    } else {
        comparison.stderr = StringComparison::TestDiffers {
            test_string: test_result.stderr.clone(),
            oracle_string: oracle_result.stderr,
        };
    }

    // Compare temporary directory contents
    comparison.temp_dir = diff_dirs(oracle_temp_dir.path(), test_temp_dir.path())?;

    Ok((comparison, test_result))
}

fn check_expectations(
    test_case_set: &TestCaseSet,
    test_case: &TestCase,
    test_result: &crate::execution::RunResult,
    test_temp_dir: Option<&assert_fs::TempDir>,
    config: &RunnerConfig,
) -> ExpectationComparison {
    let mut comparison = ExpectationComparison::not_specified();

    // Check inline expectations
    if let Some(expected_exit_code) = test_case.expected_exit_code {
        let actual_code = test_result.exit_status.code().unwrap_or(-1);
        if actual_code == expected_exit_code {
            comparison.exit_code = SingleExpectationComparison::Matches;
        } else {
            comparison.exit_code = SingleExpectationComparison::Differs {
                expected: expected_exit_code.to_string(),
                actual: actual_code.to_string(),
            };
        }
    }

    if let Some(expected_stdout) = &test_case.expected_stdout {
        if output_matches(
            expected_stdout,
            &test_result.stdout,
            test_case.ignore_whitespace,
        ) {
            comparison.stdout = SingleExpectationComparison::Matches;
        } else {
            comparison.stdout = SingleExpectationComparison::Differs {
                expected: expected_stdout.clone(),
                actual: test_result.stdout.clone(),
            };
        }
    }

    if let Some(expected_stderr) = &test_case.expected_stderr {
        if output_matches(
            expected_stderr,
            &test_result.stderr,
            test_case.ignore_whitespace,
        ) {
            comparison.stderr = SingleExpectationComparison::Matches;
        } else {
            comparison.stderr = SingleExpectationComparison::Differs {
                expected: expected_stderr.clone(),
                actual: test_result.stderr.clone(),
            };
        }
    }

    // Check snapshot if enabled
    if test_case.snapshot {
        comparison.snapshot_used = true;
        comparison.snapshot_result = Some(check_snapshot(
            test_case_set,
            test_case,
            test_result,
            test_temp_dir,
            config,
        ));
    }

    comparison
}

#[cfg(feature = "insta")]
fn check_snapshot(
    test_case_set: &TestCaseSet,
    test_case: &TestCase,
    test_result: &crate::execution::RunResult,
    test_temp_dir: Option<&assert_fs::TempDir>,
    config: &RunnerConfig,
) -> SnapshotResult {
    // Collect files from the temp directory
    let files = collect_temp_dir_files(test_temp_dir, test_case);

    // Build snapshot content manually with literal block style for better readability
    let snapshot_content = format_snapshot_yaml(
        test_result.exit_status.code().unwrap_or(-1),
        &test_result.stdout,
        &test_result.stderr,
        &files,
    );

    // Compute snapshot path
    let snapshot_dir = test_case_set.source_dir.join(&config.snapshot_dir_name);
    let yaml_stem = test_case_set
        .source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let test_name = test_case.name.as_deref().unwrap_or("unnamed");

    // Convert spaces to underscores in snapshot name for filesystem compatibility
    let snapshot_name = format!("{}_{}", yaml_stem, test_name.replace(' ', "_"));

    // Use insta's settings to configure snapshot location
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(&snapshot_dir);
    settings.set_prepend_module_to_snapshot(false);

    settings.bind(|| {
        // Use assert_snapshot with raw string for block-style YAML
        let result = std::panic::catch_unwind(|| {
            insta::assert_snapshot!(snapshot_name.clone(), snapshot_content);
        });

        match result {
            Ok(()) => SnapshotResult::Matches,
            Err(_) => SnapshotResult::Differs {
                message: format!(
                    "Snapshot '{snapshot_name}' differs or is new. Run `cargo insta review` to update."
                ),
            },
        }
    })
}

/// Format snapshot data as YAML with literal block style for multiline strings.
#[cfg(feature = "insta")]
fn format_snapshot_yaml(
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    files: &std::collections::BTreeMap<String, String>,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();

    writeln!(output, "exit_code: {exit_code}").ok();

    // Format stdout
    write!(output, "stdout: ").ok();
    format_yaml_string(&mut output, stdout, 0);

    // Format stderr
    write!(output, "stderr: ").ok();
    format_yaml_string(&mut output, stderr, 0);

    // Format files if any
    if !files.is_empty() {
        writeln!(output, "files:").ok();
        for (filename, contents) in files {
            write!(output, "  {filename}: ").ok();
            format_yaml_string(&mut output, contents, 2);
        }
    }

    output
}

/// Format a string as YAML, using literal block style for multiline content.
#[cfg(feature = "insta")]
fn format_yaml_string(output: &mut String, s: &str, indent: usize) {
    use std::fmt::Write;

    if s.is_empty() {
        writeln!(output, "\"\"").ok();
    } else if s.contains('\n') {
        // Use literal block style for multiline strings
        if s.ends_with('\n') {
            writeln!(output, "|").ok();
        } else {
            writeln!(output, "|-").ok();
        }
        let indent_str = " ".repeat(indent + 2);
        for line in s.lines() {
            writeln!(output, "{indent_str}{line}").ok();
        }
        // If string ends with newline but lines() doesn't capture trailing empty line
        if s.ends_with('\n') && !s.ends_with("\n\n") {
            // Already handled by literal block indicator '|'
        } else if s.ends_with("\n\n") {
            // Multiple trailing newlines need explicit empty lines
            let trailing_newlines = s.len() - s.trim_end_matches('\n').len();
            for _ in 1..trailing_newlines {
                writeln!(output, "{indent_str}").ok();
            }
        }
    } else {
        // Single line - use quoted style if contains special chars, otherwise plain
        if s.contains(':')
            || s.contains('#')
            || s.contains('\'')
            || s.contains('"')
            || s.starts_with(' ')
            || s.ends_with(' ')
            || s == "true"
            || s == "false"
            || s == "null"
            || s.parse::<f64>().is_ok()
        {
            // Use double-quoted style with escapes
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\t', "\\t");
            writeln!(output, "\"{escaped}\"").ok();
        } else {
            writeln!(output, "{s}").ok();
        }
    }
}

/// Collect files from the temp directory, excluding test input files.
/// Returns a map of filename -> contents (as string for text files).
#[cfg(feature = "insta")]
fn collect_temp_dir_files(
    test_temp_dir: Option<&assert_fs::TempDir>,
    test_case: &TestCase,
) -> std::collections::BTreeMap<String, String> {
    use std::collections::BTreeMap;

    let mut files = BTreeMap::new();

    let Some(temp_dir) = test_temp_dir else {
        return files;
    };

    // Get the set of input test file names to exclude
    let input_files: std::collections::HashSet<_> = test_case
        .test_files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    // Walk the temp directory and collect files
    let Ok(entries) = std::fs::read_dir(temp_dir.path()) else {
        return files;
    };

    for entry in entries.flatten() {
        let filename = entry.file_name().to_string_lossy().to_string();

        // Skip input test files
        if input_files.contains(&filename) {
            continue;
        }

        // Skip coverage profile data
        if filename.ends_with(".profraw") {
            continue;
        }

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Read file contents - use lossy conversion for binary files
        if let Ok(contents) = std::fs::read(&path) {
            let text = String::from_utf8_lossy(&contents).to_string();
            files.insert(filename, text);
        }
    }

    files
}

#[cfg(not(feature = "insta"))]
fn check_snapshot(
    _test_case_set: &TestCaseSet,
    _test_case: &TestCase,
    _test_result: &crate::execution::RunResult,
    _test_temp_dir: Option<&assert_fs::TempDir>,
    _config: &RunnerConfig,
) -> SnapshotResult {
    SnapshotResult::Differs {
        message: String::from("Snapshot testing requires the 'insta' feature to be enabled"),
    }
}
