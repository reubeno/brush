//! The test harness for brush shell integration tests.

// Only compile this for Unix-like platforms (Linux, macOS) because they have an oracle to compare
// against.
#![cfg(any(unix, windows))]

use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
use clap::Parser;
use colored::Colorize;
use descape::UnescapeExt;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::ExitStatusExt};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::ExitStatus,
};
use std::{fs, io::Read};

#[derive(Clone)]
struct ShellConfig {
    pub which: WhichShell,
    pub default_args: Vec<String>,
    pub default_path_var: Option<String>,
}

impl ShellConfig {
    #[expect(clippy::unnecessary_wraps)]
    fn compute_test_path_var(&self) -> Result<String> {
        let mut dirs = vec![];

        // Start with any default we were provided.
        if let Some(default_path_var) = &self.default_path_var {
            dirs.extend(
                std::env::split_paths(default_path_var).map(|p| p.to_string_lossy().to_string()),
            );
        }

        // Add hard-coded paths that will work on *most* Unix-like systems.
        dirs.extend([
            "/usr/local/sbin".into(),
            "/usr/local/bin".into(),
            "/usr/sbin".into(),
            "/usr/bin".into(),
            "/sbin".into(),
            "/bin".into(),
        ]);

        //
        // Handle systems that are more... interesting (i.e., store their standard
        // POSIX binaries elsewhere). As a concrete example, NixOS has an interesting
        // set of paths that must be consulted, and unfortunately doesn't accurately
        // return them via confstr(). For these systems, we go through the host's
        // PATH variable, and include any paths that contain 'sh'.
        //

        if let Some(host_path) = std::env::var_os("PATH") {
            for path in std::env::split_paths(&host_path) {
                let path_str = path.to_string_lossy().to_string();
                if !dirs.contains(&path_str) && path.join("sh").is_file() {
                    dirs.push(path_str);
                }
            }
        }

        Ok(dirs.join(":"))
    }
}

#[derive(Clone)]
struct TestConfig {
    pub name: String,
    pub oracle_shell: ShellConfig,
    pub oracle_version_str: Option<String>,
    pub test_shell: ShellConfig,
    pub options: TestOptions,
}

impl TestConfig {
    pub fn for_bash_testing(options: &TestOptions) -> Result<Self> {
        // Check for bash version.
        let bash_version_str = get_bash_version_str(Path::new(&options.bash_path))?;
        if options.verbose {
            eprintln!("Detected bash version: {bash_version_str}");
        }

        // Skip rc file and profile for deterministic behavior across systems/distros.
        Ok(Self {
            name: String::from(BASH_CONFIG_NAME),
            oracle_shell: ShellConfig {
                which: WhichShell::NamedShell(options.bash_path.clone()),
                default_args: vec![String::from("--norc"), String::from("--noprofile")],
                default_path_var: options.test_path_var.clone(),
            },
            oracle_version_str: Some(bash_version_str),
            test_shell: ShellConfig {
                which: WhichShell::ShellUnderTest(PathBuf::from(&options.brush_path)),
                // Disable a few fancy UI options for shells under test.
                default_args: vec![
                    "--norc".into(),
                    "--noprofile".into(),
                    "--input-backend=basic".into(),
                    "--disable-bracketed-paste".into(),
                    "--disable-color".into(),
                ],
                default_path_var: options.test_path_var.clone(),
            },
            options: options.clone(),
        })
    }

    #[expect(clippy::unnecessary_wraps)]
    pub fn for_sh_testing(options: &TestOptions) -> Result<Self> {
        // Skip rc file and profile for deterministic behavior across systems/distros.
        Ok(Self {
            name: String::from(SH_CONFIG_NAME),
            oracle_shell: ShellConfig {
                which: WhichShell::NamedShell(PathBuf::from("sh")),
                default_args: vec![],
                default_path_var: options.test_path_var.clone(),
            },
            oracle_version_str: None,
            test_shell: ShellConfig {
                which: WhichShell::ShellUnderTest(PathBuf::from(&options.brush_path)),
                // Disable a few fancy UI options for shells under test.
                default_args: vec![
                    String::from("--sh"),
                    String::from("--norc"),
                    String::from("--noprofile"),
                    String::from("--disable-bracketed-paste"),
                ],
                default_path_var: options.test_path_var.clone(),
            },
            options: options.clone(),
        })
    }
}

async fn cli_integration_tests(mut options: TestOptions) -> Result<()> {
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut known_failure_count = 0;
    let mut fail_count = 0;
    let mut join_handles = vec![];
    let mut success_duration_comparison = DurationComparison {
        oracle: std::time::Duration::default(),
        test: std::time::Duration::default(),
    };

    // Resolve paths.
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases"),
        |p| p.to_owned(),
    );

    // Resolve path to the shell-under-test.
    if options.brush_path.is_empty() {
        options.brush_path = assert_cmd::cargo::cargo_bin("brush")
            .to_string_lossy()
            .to_string();
    }
    if !Path::new(&options.brush_path).exists() {
        return Err(anyhow::anyhow!(
            "brush binary not found: {}",
            options.brush_path
        ));
    }

    // Set up test configs
    let mut test_configs = vec![];
    if options.should_enable_config(BASH_CONFIG_NAME) {
        test_configs.push(TestConfig::for_bash_testing(&options)?);
    }
    if options.should_enable_config(SH_CONFIG_NAME) {
        test_configs.push(TestConfig::for_sh_testing(&options)?);
    }

    // Generate a glob pattern to find all the YAML test case files.
    let glob_pattern = test_cases_dir
        .join("**/*.yaml")
        .to_string_lossy()
        .to_string();

    if options.verbose {
        eprintln!("Running test cases: {glob_pattern}");
    }

    // Spawn each test case set separately.
    for entry in glob::glob(glob_pattern.as_ref()).unwrap() {
        let entry = entry.unwrap();

        let yaml_file = std::fs::File::open(entry.as_path())?;
        let mut test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)
            .context(format!("parsing {}", entry.to_string_lossy()))?;

        test_case_set.source_dir = entry.parent().unwrap().to_path_buf();

        for test_config in &test_configs {
            // Make sure it's compatible.
            if test_case_set
                .incompatible_configs
                .contains(&test_config.name)
            {
                continue;
            }

            if options.list_tests_only {
                for test_case in &test_case_set.cases {
                    let case_is_skipped = test_case.should_skip(&test_case_set, test_config)?;
                    if case_is_skipped == options.skipped_tests_only {
                        println!(
                            "{}::{}: test",
                            test_case_set.name.as_deref().unwrap_or("unnamed"),
                            test_case.name.as_deref().unwrap_or("unnamed"),
                        );
                    }
                }
            } else {
                // Clone the test case set and test config so the spawned function below
                // can take ownership of the clones.
                let test_case_set = test_case_set.clone();
                let test_config = test_config.clone();

                join_handles.push(tokio::spawn(
                    async move { test_case_set.run(test_config).await },
                ));
            }
        }
    }

    // Now go through and await everything.
    let mut all_results = vec![];
    for join_handle in join_handles {
        let results = join_handle.await??;

        success_count += results.success_count;
        skip_count += results.skip_count;
        known_failure_count += results.known_failure_count;
        fail_count += results.fail_count;
        success_duration_comparison.oracle += results.success_duration_comparison.oracle;
        success_duration_comparison.test += results.success_duration_comparison.test;

        all_results.push(results);
    }

    if options.list_tests_only {
        return Ok(());
    }

    report_integration_test_results(all_results, &options).unwrap();

    if matches!(options.format, OutputFormat::Pretty) {
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
        eprintln!(
            "duration of successful tests: {:?} (oracle) vs. {:?} (test)",
            success_duration_comparison.oracle, success_duration_comparison.test,
        );
        eprintln!(
            "================================================================================"
        );
    }

    assert!(fail_count == 0);

    Ok(())
}

fn report_integration_test_results(
    results: Vec<TestCaseSetResults>,
    options: &TestOptions,
) -> Result<()> {
    match options.format {
        OutputFormat::Pretty => report_integration_test_results_pretty(results, options),
        OutputFormat::Junit => report_integration_test_results_junit(results, options),
        OutputFormat::Terse => Ok(()),
    }
}

fn report_integration_test_results_junit(
    results: Vec<TestCaseSetResults>,
    options: &TestOptions,
) -> Result<()> {
    let mut report = junit_report::Report::new();

    for result in results {
        let mut suite = junit_report::TestSuite::new(result.name.unwrap_or(String::new()).as_str());
        for r in result.test_case_results {
            let test_case_name = r.name.as_deref().unwrap_or("");
            let mut test_case: junit_report::TestCase = if r.success {
                junit_report::TestCase::success(
                    test_case_name,
                    r.comparison.duration.test.try_into()?,
                )
            } else if r.known_failure {
                junit_report::TestCase::skipped(test_case_name)
            } else {
                junit_report::TestCase::failure(
                    test_case_name,
                    r.comparison.duration.test.try_into()?,
                    "test failure",
                    "failed",
                )
            };

            let mut output_buf: Vec<u8> = vec![];
            r.write_details(&mut output_buf, options)?;

            // Strip out any VT100-style escape sequences; they won't be okay in the XML
            // that this turns into.
            let output_as_string = String::from_utf8(output_buf)?;
            test_case.set_system_out(strip_ansi_escapes::strip_str(output_as_string).as_str());

            suite.add_testcase(test_case);
        }

        report.add_testsuite(suite);
    }

    report.write_xml(std::io::stdout())?;
    writeln!(std::io::stdout())?;

    Ok(())
}

fn report_integration_test_results_pretty(
    results: Vec<TestCaseSetResults>,
    options: &TestOptions,
) -> Result<()> {
    for result in results {
        result.report_pretty(options)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TestCase {
    /// Name of the test case
    pub name: Option<String>,
    /// How to invoke the shell
    #[serde(default)]
    pub invocation: ShellInvocation,
    /// Command-line arguments to the shell
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment for the shell
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub pty: bool,
    #[serde(default)]
    pub stdin: Option<String>,
    #[serde(default)]
    pub ignore_exit_status: bool,
    #[serde(default)]
    pub ignore_stderr: bool,
    #[serde(default)]
    pub ignore_stdout: bool,
    #[serde(default)]
    pub ignore_whitespace: bool,
    #[serde(default)]
    pub test_files: Vec<TestFile>,
    #[serde(default)]
    pub known_failure: bool,
    #[serde(default)]
    pub incompatible_configs: HashSet<String>,
    #[serde(default)]
    pub min_oracle_version: Option<String>,
    #[serde(default)]
    pub max_oracle_version: Option<String>,
    #[serde(default)]
    pub timeout_in_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TestFile {
    /// Relative path to test file
    pub path: PathBuf,
    /// Contents to seed the file with
    #[serde(default)]
    pub contents: String,
    /// Optionally provides relative path to the source file
    /// that should be used to populate this file.
    pub source_path: Option<PathBuf>,
    /// Executable?
    #[serde(default)]
    pub executable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TestCaseSet {
    /// Name of the test case set
    pub name: Option<String>,
    /// Set of test cases
    pub cases: Vec<TestCase>,
    /// Common test files applicable to all children test cases
    #[serde(default)]
    pub common_test_files: Vec<TestFile>,
    #[serde(default)]
    pub incompatible_configs: HashSet<String>,

    #[serde(skip)]
    pub source_dir: PathBuf,
}

struct TestCaseSetResults {
    pub name: Option<String>,
    pub config_name: String,
    pub success_count: u32,
    pub skip_count: u32,
    pub known_failure_count: u32,
    pub fail_count: u32,
    pub test_case_results: Vec<TestCaseResult>,
    pub success_duration_comparison: DurationComparison,
}

impl TestCaseSetResults {
    pub fn report_pretty(&self, options: &TestOptions) -> Result<()> {
        self.write_details(std::io::stderr(), options)
    }

    fn write_details<W: std::io::Write>(&self, mut writer: W, options: &TestOptions) -> Result<()> {
        if options.verbose {
            writeln!(
                writer,
                "=================== {}: [{}/{}] ===================",
                "Running test case set".blue(),
                self.name
                    .as_ref()
                    .map_or_else(|| "(unnamed)", |n| n.as_str())
                    .italic(),
                self.config_name.magenta(),
            )?;
        }

        for test_case_result in &self.test_case_results {
            test_case_result.report_pretty(options)?;
        }

        if options.verbose {
            writeln!(
                writer,
                "    successful cases ran in {:?} (oracle) and {:?} (test)",
                self.success_duration_comparison.oracle, self.success_duration_comparison.test
            )?;
        }

        Ok(())
    }
}

impl TestCaseSet {
    pub async fn run(&self, test_config: TestConfig) -> Result<TestCaseSetResults> {
        let mut success_count = 0;
        let mut skip_count = 0;
        let mut known_failure_count = 0;
        let mut fail_count = 0;
        let mut success_duration_comparison = DurationComparison {
            oracle: std::time::Duration::default(),
            test: std::time::Duration::default(),
        };
        let mut test_case_results = vec![];
        for test_case in &self.cases {
            let case_is_skipped = test_case.should_skip(self, &test_config)?;
            let test_case_result = if case_is_skipped == test_config.options.skipped_tests_only {
                test_case.run(self, &test_config).await?
            } else {
                TestCaseResult {
                    success: true,
                    comparison: RunComparison::ignored(),
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
                    success_duration_comparison.oracle +=
                        test_case_result.comparison.duration.oracle;
                    success_duration_comparison.test += test_case_result.comparison.duration.test;
                }
            } else if test_case.known_failure {
                known_failure_count += 1;
            } else {
                fail_count += 1;
            }

            test_case_results.push(test_case_result);
        }

        Ok(TestCaseSetResults {
            name: self.name.clone(),
            config_name: test_config.name.clone(),
            test_case_results,
            success_count,
            skip_count,
            known_failure_count,
            fail_count,
            success_duration_comparison,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
enum ShellInvocation {
    #[default]
    ExecShellBinary,
    ExecScript(String),
}

#[derive(Clone)]
enum WhichShell {
    ShellUnderTest(PathBuf),
    NamedShell(PathBuf),
}

struct TestCaseResult {
    pub name: Option<String>,
    pub success: bool,
    pub skip: bool,
    pub known_failure: bool,
    pub comparison: RunComparison,
}

impl TestCaseResult {
    pub fn report_pretty(&self, options: &TestOptions) -> Result<()> {
        self.write_details(std::io::stderr(), options)
    }

    #[expect(clippy::too_many_lines)]
    pub fn write_details<W: std::io::Write>(
        &self,
        mut writer: W,
        options: &TestOptions,
    ) -> Result<()> {
        if self.skip {
            return Ok(());
        }

        if !options.verbose {
            if (!self.comparison.is_failure() && !self.known_failure)
                || (self.comparison.is_failure() && self.known_failure)
            {
                return Ok(());
            }
        }

        write!(
            writer,
            "* {}: [{}]... ",
            "Test case".bright_yellow(),
            self.name
                .as_ref()
                .map_or_else(|| "(unnamed)", |n| n.as_str())
                .italic()
        )?;

        if !self.comparison.is_failure() {
            if self.known_failure {
                writeln!(writer, "{}", "unexpected success.".bright_red())?;
            } else {
                writeln!(writer, "{}", "ok.".bright_green())?;
                return Ok(());
            }
        } else if self.known_failure {
            writeln!(writer, "{}", "known failure.".bright_magenta())?;
            if !options.display_known_failure_details {
                return Ok(());
            }
        }

        writeln!(writer)?;

        match self.comparison.exit_status {
            ExitStatusComparison::Ignored => writeln!(writer, "    status {}", "ignored".cyan())?,
            ExitStatusComparison::Same(status) => {
                writeln!(
                    writer,
                    "    status matches ({}) {}",
                    format!("{status}").green(),
                    "✔️".green()
                )?;
            }
            ExitStatusComparison::TestDiffers {
                test_exit_status,
                oracle_exit_status,
            } => {
                writeln!(
                    writer,
                    "    status mismatch: {} from oracle vs. {} from test",
                    format!("{oracle_exit_status}").cyan(),
                    format!("{test_exit_status}").bright_red()
                )?;
            }
        }

        match &self.comparison.stdout {
            StringComparison::Ignored {
                test_string,
                oracle_string,
            } => {
                writeln!(writer, "    stdout {}", "ignored".cyan())?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Oracle: stdout ---------------------------------".cyan()
                )?;
                writeln!(writer, "{}", indent::indent_all_by(8, oracle_string))?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Oracle: stdout [cleaned]------------------------".cyan()
                )?;
                writeln!(
                    writer,
                    "{}",
                    indent::indent_all_by(8, make_expectrl_output_readable(oracle_string))
                )?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Test: stdout ---------------------------------".cyan()
                )?;
                writeln!(writer, "{}", indent::indent_all_by(8, test_string))?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Test: stdout [cleaned]------------------------".cyan()
                )?;

                writeln!(
                    writer,
                    "{}",
                    indent::indent_all_by(8, make_expectrl_output_readable(test_string))
                )?;
            }
            StringComparison::Same(s) => {
                writeln!(writer, "    stdout matches {}", "✔️".green())?;

                if options.verbose {
                    writeln!(
                        writer,
                        "        {}",
                        "------ Oracle <> Test: stdout ---------------------------------".cyan()
                    )?;

                    writeln!(writer, "{}", indent::indent_all_by(8, s))?;
                }
            }
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                writeln!(writer, "    stdout {}", "DIFFERS:".bright_red())?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Oracle <> Test: stdout ---------------------------------".cyan()
                )?;

                write_diff(&mut writer, 8, o.as_str(), t.as_str())?;

                writeln!(
                    writer,
                    "        {}",
                    "---------------------------------------------------------------".cyan()
                )?;
            }
        }

        match &self.comparison.stderr {
            StringComparison::Ignored {
                test_string: _,
                oracle_string: _,
            } => writeln!(writer, "    stderr {}", "ignored".cyan())?,
            StringComparison::Same(s) => {
                writeln!(writer, "    stderr matches {}", "✔️".green())?;

                if options.verbose {
                    writeln!(
                        writer,
                        "        {}",
                        "------ Oracle <> Test: stderr ---------------------------------".cyan()
                    )?;

                    writeln!(writer, "{}", indent::indent_all_by(8, s))?;
                }
            }
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                writeln!(writer, "    stderr {}", "DIFFERS:".bright_red())?;

                writeln!(
                    writer,
                    "        {}",
                    "------ Oracle <> Test: stderr ---------------------------------".cyan()
                )?;

                write_diff(&mut writer, 8, o.as_str(), t.as_str())?;

                writeln!(
                    writer,
                    "        {}",
                    "---------------------------------------------------------------".cyan()
                )?;
            }
        }

        match &self.comparison.temp_dir {
            DirComparison::Ignored => writeln!(writer, "    temp dir {}", "ignored".cyan())?,
            DirComparison::Same => writeln!(writer, "    temp dir matches {}", "✔️".green())?,
            DirComparison::TestDiffers(entries) => {
                writeln!(writer, "    temp dir {}", "DIFFERS".bright_red())?;

                for entry in entries {
                    const INDENT: &str = "        ";
                    match entry {
                        DirComparisonEntry::Different(
                            left_path,
                            left_contents,
                            right_path,
                            right_contents,
                        ) => {
                            writeln!(
                                writer,
                                "{INDENT}oracle file {} differs from test file {}",
                                left_path.to_string_lossy(),
                                right_path.to_string_lossy()
                            )?;

                            writeln!(
                                writer,
                                "{INDENT}{}",
                                "------ Oracle <> Test: file ---------------------------------"
                                    .cyan()
                            )?;

                            write_diff(
                                &mut writer,
                                8,
                                left_contents.as_str(),
                                right_contents.as_str(),
                            )?;

                            writeln!(
                                writer,
                                "        {}",
                                "---------------------------------------------------------------"
                                    .cyan()
                            )?;
                        }
                        DirComparisonEntry::LeftOnly(p) => {
                            writeln!(
                                writer,
                                "{INDENT}file missing from test dir: {}",
                                p.to_string_lossy()
                            )?;
                        }
                        DirComparisonEntry::RightOnly(p) => {
                            writeln!(
                                writer,
                                "{INDENT}unexpected file in test dir: {}",
                                p.to_string_lossy()
                            )?;
                        }
                    }
                }
            }
        }

        if !self.success {
            writeln!(writer, "    {}", "FAILED.".bright_red())?;
        }

        Ok(())
    }
}

impl TestCase {
    pub async fn run(
        &self,
        test_case_set: &TestCaseSet,
        test_config: &TestConfig,
    ) -> Result<TestCaseResult> {
        let comparison = self
            .run_with_oracle_and_test(test_case_set, test_config)
            .await?;
        let success = !comparison.is_failure();
        Ok(TestCaseResult {
            success,
            comparison,
            name: self.name.clone(),
            skip: false,
            known_failure: self.known_failure,
        })
    }

    pub fn should_skip(
        &self,
        test_case_set: &TestCaseSet,
        test_config: &TestConfig,
    ) -> Result<bool> {
        // Make sure it's compatible.
        if self.incompatible_configs.contains(&test_config.name) {
            return Ok(true);
        }

        // Make sure the oracle meets any version constraints listed.
        if self.min_oracle_version.is_some() || self.max_oracle_version.is_some() {
            if let Some(actual_oracle_version_str) = &test_config.oracle_version_str {
                let actual_oracle_version =
                    version_compare::Version::from(actual_oracle_version_str.as_str())
                        .ok_or_else(|| anyhow::anyhow!("failed to parse oracle version"))?;

                if let Some(min_oracle_version_str) = &self.min_oracle_version {
                    let min_oracle_version = version_compare::Version::from(min_oracle_version_str)
                        .ok_or_else(|| anyhow::anyhow!("failed to parse min oracle version"))?;

                    if matches!(
                        actual_oracle_version.compare(min_oracle_version),
                        version_compare::Cmp::Lt
                    ) {
                        return Ok(true);
                    }
                }

                if let Some(max_oracle_version_str) = &self.max_oracle_version {
                    let max_oracle_version = version_compare::Version::from(max_oracle_version_str)
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

        // Make sure it passes filters.
        if !test_config.options.should_run_test(test_case_set, self) {
            return Ok(true);
        }

        Ok(self.skip)
    }

    fn create_test_files_in(
        &self,
        temp_dir: &assert_fs::TempDir,
        test_case_set: &TestCaseSet,
    ) -> Result<()> {
        for test_file in test_case_set
            .common_test_files
            .iter()
            .chain(self.test_files.iter())
        {
            let test_file_path = temp_dir.child(test_file.path.as_path());

            if let Some(source_path) = &test_file.source_path {
                if !test_file.contents.is_empty() {
                    return Err(anyhow::anyhow!(
                        "test file {} has both contents and source_path",
                        test_file_path.to_string_lossy()
                    ));
                }

                if source_path.is_absolute() {
                    return Err(anyhow::anyhow!(
                        "source_path {} is not a relative path",
                        source_path.to_string_lossy()
                    ));
                }

                let abs_source_path = test_case_set.source_dir.join(source_path);

                let source_contents = std::fs::read_to_string(&abs_source_path)
                    .with_context(|| format!("reading {}", abs_source_path.to_string_lossy()))?;

                test_file_path.write_str(source_contents.as_str())?;
            } else {
                test_file_path.write_str(test_file.contents.as_str())?;
            }

            #[cfg(unix)]
            if test_file.executable {
                // chmod u+x
                let mut perms = test_file_path.metadata()?.permissions();
                perms.set_mode(perms.mode() | 0o100);
                std::fs::set_permissions(test_file_path, perms)?;
            }
        }

        Ok(())
    }

    async fn run_with_oracle_and_test(
        &self,
        test_case_set: &TestCaseSet,
        test_config: &TestConfig,
    ) -> Result<RunComparison> {
        let oracle_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&oracle_temp_dir, test_case_set)?;
        let oracle_result = self
            .run_shell(&test_config.oracle_shell, &oracle_temp_dir)
            .await?;

        let test_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&test_temp_dir, test_case_set)?;
        let test_result = self
            .run_shell(&test_config.test_shell, &test_temp_dir)
            .await?;

        let mut comparison = RunComparison {
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
        if self.ignore_exit_status {
            comparison.exit_status = ExitStatusComparison::Ignored;
        } else if oracle_result.exit_status == test_result.exit_status {
            comparison.exit_status = ExitStatusComparison::Same(oracle_result.exit_status);
        } else {
            comparison.exit_status = ExitStatusComparison::TestDiffers {
                test_exit_status: test_result.exit_status,
                oracle_exit_status: oracle_result.exit_status,
            }
        }

        // Compare stdout
        if self.ignore_stdout {
            comparison.stdout = StringComparison::Ignored {
                test_string: test_result.stdout,
                oracle_string: oracle_result.stdout,
            };
        } else if self.output_matches(&oracle_result.stdout, &test_result.stdout) {
            comparison.stdout = StringComparison::Same(oracle_result.stdout);
        } else {
            comparison.stdout = StringComparison::TestDiffers {
                test_string: test_result.stdout,
                oracle_string: oracle_result.stdout,
            }
        }

        // Compare stderr
        if self.ignore_stderr {
            comparison.stderr = StringComparison::Ignored {
                test_string: test_result.stderr,
                oracle_string: oracle_result.stderr,
            };
        } else if self.output_matches(&oracle_result.stderr, &test_result.stderr) {
            comparison.stderr = StringComparison::Same(oracle_result.stderr);
        } else {
            comparison.stderr = StringComparison::TestDiffers {
                test_string: test_result.stderr,
                oracle_string: oracle_result.stderr,
            }
        }

        // Compare temporary directory contents
        comparison.temp_dir = diff_dirs(oracle_temp_dir.path(), test_temp_dir.path())?;

        Ok(comparison)
    }

    async fn run_shell(
        &self,
        shell_config: &ShellConfig,
        working_dir: &assert_fs::TempDir,
    ) -> Result<RunResult> {
        let test_cmd = self.create_command_for_shell(shell_config, working_dir);

        let result = if self.pty {
            self.run_command_with_pty(test_cmd).await?
        } else {
            self.run_command_with_stdin(test_cmd).await?
        };

        Ok(result)
    }

    fn create_command_for_shell(
        &self,
        shell_config: &ShellConfig,
        working_dir: &assert_fs::TempDir,
    ) -> std::process::Command {
        let (mut test_cmd, coverage_target_dir) = match self.invocation {
            ShellInvocation::ExecShellBinary => match &shell_config.which {
                WhichShell::ShellUnderTest(name) => {
                    let cli_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                    let default_target_dir = || cli_dir.parent().unwrap().join("target");
                    let target_dir = std::env::var("CARGO_TARGET_DIR")
                        .ok()
                        .map_or_else(default_target_dir, PathBuf::from);
                    (std::process::Command::new(name), Some(target_dir))
                }
                WhichShell::NamedShell(name) => (std::process::Command::new(name), None),
            },
            ShellInvocation::ExecScript(_) => unimplemented!("exec script test"),
        };

        for arg in &shell_config.default_args {
            test_cmd.arg(arg);
        }

        // Clear all environment vars for consistency.
        test_cmd.args(&self.args).env_clear();

        // Hard-code a well known prompt for PS1.
        test_cmd.env("PS1", "test$ ");
        // Try to get decent backtraces when problems get hit.
        test_cmd.env("RUST_BACKTRACE", "1");
        // Compute a PATH that contains what we need.
        test_cmd.env("PATH", shell_config.compute_test_path_var().unwrap());

        // Set up any env vars needed for collecting coverage data.
        if let Some(coverage_target_dir) = &coverage_target_dir {
            test_cmd.env("CARGO_LLVM_COV_TARGET_DIR", coverage_target_dir);
            test_cmd.env(
                "LLVM_PROFILE_FILE",
                coverage_target_dir.join("brush-%p-%40m.profraw"),
            );
        }

        for (k, v) in &self.env {
            test_cmd.env(k, v);
        }

        test_cmd.current_dir(working_dir.to_string_lossy().to_string());

        test_cmd
    }

    #[expect(clippy::unused_async)]
    #[cfg(not(unix))]
    async fn run_command_with_pty(&self, _cmd: std::process::Command) -> Result<RunResult> {
        Err(anyhow::anyhow!("pty test not supported on this platform"))
    }

    #[expect(clippy::unused_async)]
    #[cfg(unix)]
    async fn run_command_with_pty(&self, cmd: std::process::Command) -> Result<RunResult> {
        use expectrl::Expect;

        let mut log = Vec::new();
        let writer = std::io::Cursor::new(&mut log);

        let start_time = std::time::Instant::now();
        let mut p = expectrl::session::log(expectrl::Session::spawn(cmd)?, writer)?;

        if let Some(stdin) = &self.stdin {
            for line in stdin.lines() {
                if let Some(expectation) = line.strip_prefix("#expect:") {
                    if let Err(inner) = p.expect(expectation) {
                        return Ok(RunResult {
                            exit_status: ExitStatus::from_raw(1),
                            stdout: read_expectrl_log(log).unwrap_or_default(),
                            stderr: std::format!("failed to expect '{expectation}': {inner}"),
                            duration: start_time.elapsed(),
                        });
                    }
                } else if let Some(control_code) = line.strip_prefix("#send:") {
                    match control_code.to_lowercase().as_str() {
                        "ctrl+d" => p.send(expectrl::ControlCode::EndOfTransmission)?,
                        "tab" => p.send(expectrl::ControlCode::HorizontalTabulation)?,
                        "enter" => p.send(expectrl::ControlCode::LineFeed)?,
                        _ => (),
                    }
                } else if line.trim() == "#expect-prompt" {
                    if let Err(inner) = p.expect("test$ ") {
                        return Ok(RunResult {
                            exit_status: ExitStatus::from_raw(1),
                            stdout: read_expectrl_log(log).unwrap_or_default(),
                            stderr: std::format!("failed to expect prompt: {inner}"),
                            duration: start_time.elapsed(),
                        });
                    }
                } else {
                    p.send(line)?;
                }
            }
        }

        if let Err(inner) = p.expect(expectrl::Eof) {
            return Ok(RunResult {
                exit_status: ExitStatus::from_raw(1),
                stdout: read_expectrl_log(log).unwrap_or_default(),
                stderr: std::format!("failed to expect EOF: {inner}"),
                duration: start_time.elapsed(),
            });
        }

        let mut wait_status = p.get_process().status()?;

        if matches!(wait_status, expectrl::process::unix::WaitStatus::StillAlive) {
            // Try to terminate it safely.
            p.get_process_mut()
                .kill(expectrl::process::unix::Signal::SIGTERM)?;
            wait_status = p.get_process().wait()?;
        }

        let duration = start_time.elapsed();
        let output = read_expectrl_log(log)?;
        let cleaned = make_expectrl_output_readable(output);

        match wait_status {
            expectrl::process::unix::WaitStatus::Exited(_, code) => Ok(RunResult {
                exit_status: ExitStatus::from_raw(code),
                stdout: cleaned,
                stderr: String::new(),
                duration,
            }),
            expectrl::process::unix::WaitStatus::Signaled(_, _, _) => {
                Err(anyhow::anyhow!("process was signaled"))
            }
            _ => Err(anyhow::anyhow!(
                "unexpected status for process: {wait_status:?}"
            )),
        }
    }

    #[expect(clippy::unused_async)]
    async fn run_command_with_stdin(&self, cmd: std::process::Command) -> Result<RunResult> {
        const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;

        let mut test_cmd = assert_cmd::Command::from_std(cmd);

        test_cmd.timeout(std::time::Duration::from_secs(
            self.timeout_in_seconds
                .unwrap_or(DEFAULT_TIMEOUT_IN_SECONDS),
        ));

        if let Some(stdin) = &self.stdin {
            test_cmd.write_stdin(stdin.as_bytes());
        }

        let start_time = std::time::Instant::now();
        let cmd_result = test_cmd.output()?;
        let duration = start_time.elapsed();

        Ok(RunResult {
            exit_status: cmd_result.status,
            stdout: String::from_utf8_lossy(cmd_result.stdout.as_slice()).to_string(),
            stderr: String::from_utf8_lossy(cmd_result.stderr.as_slice()).to_string(),
            duration,
        })
    }

    fn output_matches<S: AsRef<str>>(&self, oracle: S, test: S) -> bool {
        if self.ignore_whitespace {
            let whitespace_re = regex::Regex::new(r"\s+").unwrap();

            let cleaned_oracle = whitespace_re.replace_all(oracle.as_ref(), " ").to_string();
            let cleaned_test = whitespace_re.replace_all(test.as_ref(), " ").to_string();

            cleaned_oracle == cleaned_test
        } else {
            oracle.as_ref() == test.as_ref()
        }
    }
}

struct RunResult {
    pub exit_status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub duration: std::time::Duration,
}

struct RunComparison {
    pub exit_status: ExitStatusComparison,
    pub stdout: StringComparison,
    pub stderr: StringComparison,
    pub temp_dir: DirComparison,
    pub duration: DurationComparison,
}

impl RunComparison {
    pub const fn is_failure(&self) -> bool {
        self.exit_status.is_failure()
            || self.stdout.is_failure()
            || self.stderr.is_failure()
            || self.temp_dir.is_failure()
    }

    pub fn ignored() -> Self {
        Self {
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
                oracle: std::time::Duration::default(),
                test: std::time::Duration::default(),
            },
        }
    }
}

struct DurationComparison {
    pub oracle: std::time::Duration,
    pub test: std::time::Duration,
}

enum ExitStatusComparison {
    Ignored,
    Same(ExitStatus),
    TestDiffers {
        test_exit_status: ExitStatus,
        oracle_exit_status: ExitStatus,
    },
}

impl ExitStatusComparison {
    pub const fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::TestDiffers {
                test_exit_status: _,
                oracle_exit_status: _
            }
        )
    }
}

enum StringComparison {
    Ignored {
        test_string: String,
        oracle_string: String,
    },
    Same(String),
    TestDiffers {
        test_string: String,
        oracle_string: String,
    },
}

impl StringComparison {
    pub const fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::TestDiffers {
                test_string: _,
                oracle_string: _
            }
        )
    }
}

enum DirComparisonEntry {
    LeftOnly(PathBuf),
    RightOnly(PathBuf),
    Different(PathBuf, String, PathBuf, String),
}

enum DirComparison {
    Ignored,
    Same,
    TestDiffers(Vec<DirComparisonEntry>),
}

impl DirComparison {
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::TestDiffers(_))
    }
}

fn get_dir_entries(dir_path: &Path) -> Result<HashMap<String, std::fs::FileType>> {
    let mut entries = HashMap::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let filename = entry.file_name().to_string_lossy().to_string();

        // N.B. We ignore raw coverage profile data files.
        if filename.ends_with(".profraw") {
            continue;
        }

        entries.insert(filename, file_type);
    }

    Ok(entries)
}

fn diff_dirs(oracle_path: &Path, test_path: &Path) -> Result<DirComparison> {
    let mut entries = vec![];

    let oracle_entries = get_dir_entries(oracle_path)?;
    let test_entries = get_dir_entries(test_path)?;

    // Look through all the files in the oracle directory
    for (filename, file_type) in &oracle_entries {
        if !test_entries.contains_key(filename) {
            for left_only_file in walkdir::WalkDir::new(oracle_path.join(filename)) {
                let entry = left_only_file?;
                let left_only_path = entry.path();
                entries.push(DirComparisonEntry::LeftOnly(left_only_path.to_owned()));
            }

            continue;
        }

        let oracle_file_path = oracle_path.join(filename);
        let test_file_path = test_path.join(filename);

        if file_type.is_file() {
            let mut oracle_file = std::fs::OpenOptions::new()
                .read(true)
                .open(&oracle_file_path)?;
            let mut oracle_bytes = vec![];
            oracle_file.read_to_end(&mut oracle_bytes)?;

            let mut test_file = std::fs::OpenOptions::new()
                .read(true)
                .open(&test_file_path)?;
            let mut test_bytes = vec![];
            test_file.read_to_end(&mut test_bytes)?;

            if oracle_bytes != test_bytes {
                // Convert using lossy conversion to avoid issues with invalid UTF-8.
                let oracle_display_text = String::from_utf8_lossy(&oracle_bytes);
                let test_display_text = String::from_utf8_lossy(&test_bytes);

                entries.push(DirComparisonEntry::Different(
                    oracle_file_path,
                    oracle_display_text.to_string(),
                    test_file_path,
                    test_display_text.to_string(),
                ));
            }
        } else if file_type.is_dir() {
            let subdir_comparison =
                diff_dirs(oracle_file_path.as_path(), test_file_path.as_path())?;
            if let DirComparison::TestDiffers(subdir_entries) = subdir_comparison {
                entries.extend(subdir_entries);
            }
        } else {
            // Ignore other file types (e.g., symlinks).
        }
    }

    for (filename, file_type) in &test_entries {
        if oracle_entries.contains_key(filename) {
            continue;
        }

        if file_type.is_dir() {
            for right_only_file in walkdir::WalkDir::new(test_path.join(filename)) {
                let entry = right_only_file?;
                let right_only_path = entry.path();
                entries.push(DirComparisonEntry::RightOnly(right_only_path.to_owned()));
            }
        } else {
            entries.push(DirComparisonEntry::RightOnly(test_path.join(filename)));
        }
    }

    if entries.is_empty() {
        Ok(DirComparison::Same)
    } else {
        Ok(DirComparison::TestDiffers(entries))
    }
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Pretty,
    Junit,
    Terse,
}

#[derive(Clone, Parser)]
#[clap(version, about, disable_help_flag = true, disable_version_flag = true)]
struct TestOptions {
    /// Display usage information
    #[clap(long = "help", action = clap::ArgAction::HelpLong)]
    pub help: Option<bool>,

    /// Output format for test results
    #[clap(long = "format", default_value = "pretty")]
    pub format: OutputFormat,

    /// Display full details on known failures
    #[clap(long = "known-failure-details")]
    pub display_known_failure_details: bool,

    /// Display details regarding successful test cases
    #[clap(short = 'v', long = "verbose", env = "BRUSH_VERBOSE")]
    pub verbose: bool,

    /// Enable a specific configuration
    #[clap(long = "enable-config")]
    pub enabled_configs: Vec<String>,

    /// List available tests without running them
    #[clap(long = "list")]
    pub list_tests_only: bool,

    /// Exactly match filters (not just substring match)
    #[clap(long = "exact")]
    pub exact_match: bool,

    /// Optionally specify a non-default path for bash
    #[clap(long = "bash-path", default_value = "bash", env = "BASH_PATH")]
    pub bash_path: PathBuf,

    /// Optionally specify a non-default path for brush
    #[clap(long = "brush-path", default_value = "", env = "BRUSH_PATH")]
    pub brush_path: String,

    /// Optionally specify path to test cases
    #[clap(long = "test-cases-path", env = "BRUSH_COMPAT_TEST_CASES")]
    pub test_cases_path: Option<PathBuf>,

    /// Optionally specify PATH variable to use in shells
    #[clap(long = "test-path-var", env = "BRUSH_COMPAT_TEST_PATH_VAR")]
    pub test_path_var: Option<String>,

    /// Show output from test cases (for compatibility only, has no effect)
    #[clap(long = "show-output")]
    pub show_output: bool,

    /// Capture output? (for compatibility only, has no effect)
    #[clap(long = "nocapture")]
    pub no_capture: bool,

    /// Colorize output? (for compatibility only, has no effect)
    #[clap(long = "color", default_value_t = clap::ColorChoice::Auto)]
    pub color: clap::ColorChoice,

    #[clap(long = "ignored")]
    pub skipped_tests_only: bool,

    /// Unstable flags (for compatibility only, has no effect)
    #[clap(short = 'Z')]
    pub unstable_flag: Vec<String>,

    /// Patterns for tests to be excluded.
    #[clap(long = "skip")]
    pub exclude_filters: Vec<String>,

    /// Patterns for tests to be included.
    pub include_filters: Vec<String>,
}

const BASH_CONFIG_NAME: &str = "bash";
const SH_CONFIG_NAME: &str = "sh";

impl TestOptions {
    pub fn should_enable_config(&self, config: &str) -> bool {
        let enabled_configs = if self.enabled_configs.is_empty() {
            vec![String::from(BASH_CONFIG_NAME)]
        } else {
            self.enabled_configs.clone()
        };

        enabled_configs.contains(&config.to_string())
    }

    pub fn should_run_test(&self, test_case_set: &TestCaseSet, test_case: &TestCase) -> bool {
        if self.include_filters.is_empty() && self.exclude_filters.is_empty() {
            return true;
        }

        let test_case_set_name = test_case_set.name.as_deref().unwrap_or("");
        let test_case_name = test_case.name.as_deref().unwrap_or("");

        if test_case_set_name.is_empty() || test_case_name.is_empty() {
            return false;
        }

        let qualified_name = format!("{test_case_set_name}::{test_case_name}");

        // If any include filters were given, then we are in opt-in mode. We only run tests that
        // match 1 or more include filters.
        if !self.include_filters.is_empty() {
            if !self.test_matches_filters(&qualified_name, &self.include_filters) {
                return false;
            }
        }

        // In all cases, exclude filters may be used to exclude tests.
        if !self.exclude_filters.is_empty() {
            if self.test_matches_filters(&qualified_name, &self.exclude_filters) {
                return false;
            }
        }

        true
    }

    fn test_matches_filters(&self, qualified_test_name: &String, filters: &[String]) -> bool {
        // In exact match mode, filters must be an exact match; substring matches are not
        // considered.
        if self.exact_match {
            filters.contains(qualified_test_name)
        } else {
            filters
                .iter()
                .any(|filter| qualified_test_name.contains(filter))
        }
    }
}

#[cfg(unix)]
fn read_expectrl_log(log: Vec<u8>) -> Result<String> {
    let output_str = String::from_utf8(log)?;
    let output: String = output_str
        .lines()
        .filter(|line| line.starts_with("read:"))
        .map(|line| {
            line.strip_prefix("read: \"")
                .unwrap()
                .strip_suffix('"')
                .unwrap()
        })
        .collect();

    Ok(output)
}

fn make_expectrl_output_readable<S: AsRef<str>>(output: S) -> String {
    // Unescape the escaping done by expectrl's logging mechanism to get
    // back to a real string.
    let unescaped = output.as_ref().to_unescaped().unwrap().to_string();

    // And remove VT escape sequences.
    strip_ansi_escapes::strip_str(unescaped)
}

fn write_diff(
    writer: &mut impl std::io::Write,
    indent: usize,
    left: &str,
    right: &str,
) -> Result<()> {
    let indent_str = " ".repeat(indent);

    let diff = diff::lines(left, right);
    for d in diff {
        let formatted = match d {
            diff::Result::Left(l) => std::format!("{indent_str}- {l}").red(),
            diff::Result::Both(l, _) => std::format!("{indent_str}  {l}").bright_black(),
            diff::Result::Right(r) => std::format!("{indent_str}+ {r}").green(),
        };

        writeln!(writer, "{formatted}")?;
    }

    Ok(())
}

fn get_bash_version_str(bash_path: &Path) -> Result<String> {
    let output = std::process::Command::new(bash_path)
        .arg("--norc")
        .arg("--noprofile")
        .arg("-c")
        .arg("echo -n ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}.${BASH_VERSINFO[2]}")
        .output()
        .context("failed to retrieve bash version")?
        .stdout;

    let ver_str = String::from_utf8(output)?;

    Ok(ver_str)
}

fn main() -> Result<()> {
    let unparsed_args: Vec<_> = std::env::args().collect();
    let options = TestOptions::parse_from(unparsed_args);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(32)
        .build()?
        .block_on(cli_integration_tests(options))?;

    Ok(())
}
