//! The test harness for brush shell integration tests.

use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
use clap::Parser;
use colored::Colorize;
#[cfg(unix)]
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

#[derive(Clone)]
struct ShellConfig {
    pub which: WhichShell,
    pub default_args: Vec<String>,
}

#[derive(Clone)]
struct TestConfig {
    pub name: String,
    pub oracle_shell: ShellConfig,
    pub test_shell: ShellConfig,
    pub options: TestOptions,
}

impl TestConfig {
    pub fn for_bash_testing(options: &TestOptions) -> Self {
        // Skip rc file and profile for deterministic behavior across systems/distros.
        Self {
            name: String::from("bash"),
            oracle_shell: ShellConfig {
                which: WhichShell::NamedShell(String::from("bash")),
                default_args: vec![String::from("--norc"), String::from("--noprofile")],
            },
            test_shell: ShellConfig {
                which: WhichShell::ShellUnderTest(String::from("brush")),
                // Disable a few fancy UI options for shells under test.
                default_args: vec![
                    String::from("--norc"),
                    String::from("--noprofile"),
                    String::from("--disable-bracketed-paste"),
                ],
            },
            options: options.clone(),
        }
    }

    pub fn for_sh_testing(options: &TestOptions) -> Self {
        // Skip rc file and profile for deterministic behavior across systems/distros.
        Self {
            name: String::from("sh"),
            oracle_shell: ShellConfig {
                which: WhichShell::NamedShell(String::from("sh")),
                default_args: vec![],
            },
            test_shell: ShellConfig {
                which: WhichShell::ShellUnderTest(String::from("brush")),
                // Disable a few fancy UI options for shells under test.
                default_args: vec![
                    String::from("--sh"),
                    String::from("--norc"),
                    String::from("--noprofile"),
                    String::from("--disable-bracketed-paste"),
                ],
            },
            options: options.clone(),
        }
    }
}

async fn cli_integration_tests(options: TestOptions) -> Result<()> {
    let dir = env!("CARGO_MANIFEST_DIR");

    let mut success_count = 0;
    let mut skip_count = 0;
    let mut known_failure_count = 0;
    let mut fail_count = 0;
    let mut join_handles = vec![];
    let mut success_duration_comparison = DurationComparison {
        oracle: std::time::Duration::default(),
        test: std::time::Duration::default(),
    };

    let mut test_configs = vec![];

    if options.should_enable_config("bash") {
        test_configs.push(TestConfig::for_bash_testing(&options));
    }

    if options.should_enable_config("sh") {
        test_configs.push(TestConfig::for_sh_testing(&options));
    }

    // Spawn each test case set separately.
    for entry in glob::glob(format!("{dir}/tests/cases/**/*.yaml").as_ref()).unwrap() {
        let entry = entry.unwrap();

        let yaml_file = std::fs::File::open(entry.as_path())?;
        let test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)
            .context(format!("parsing {}", entry.to_string_lossy()))?;

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
                    println!(
                        "{}::{}: test",
                        test_case_set.name.as_deref().unwrap_or("unnamed"),
                        test_case.name.as_deref().unwrap_or("unnamed"),
                    );
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
            test_case.set_system_out(String::from_utf8(output_buf)?.as_str());

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
    pub timeout_in_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TestFile {
    /// Relative path to test file
    pub path: PathBuf,
    /// Contents to seed the file with
    #[serde(default)]
    pub contents: String,
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
}

#[allow(clippy::struct_field_names)]
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
                    .unwrap_or(&("(unnamed)".to_owned()))
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
            // Make sure it's compatible.
            if test_case.incompatible_configs.contains(&test_config.name) {
                continue;
            }

            // Make sure it passes filters.
            if !test_config.options.should_run_test(self, test_case) {
                continue;
            }

            let test_case_result = test_case.run(self, &test_config).await?;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
enum ShellInvocation {
    ExecShellBinary,
    ExecScript(String),
}

impl Default for ShellInvocation {
    fn default() -> Self {
        Self::ExecShellBinary
    }
}

#[derive(Clone)]
enum WhichShell {
    ShellUnderTest(String),
    NamedShell(String),
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

    #[allow(clippy::too_many_lines)]
    pub fn write_details<W: std::io::Write>(
        &self,
        mut writer: W,
        options: &TestOptions,
    ) -> Result<()> {
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
                .unwrap_or(&("(unnamed)".to_owned()))
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
            StringComparison::Same(_) => writeln!(writer, "    stdout matches {}", "✔️".green())?,
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
            StringComparison::Same(_) => writeln!(writer, "    stderr matches {}", "✔️".green())?,
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
                        DirComparisonEntry::Different(left, right) => {
                            writeln!(
                                writer,
                                "{INDENT}oracle file {} differs from test file {}",
                                left.to_string_lossy(),
                                right.to_string_lossy()
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
        if self.skip {
            return Ok(TestCaseResult {
                success: true,
                comparison: RunComparison::ignored(),
                name: self.name.clone(),
                skip: true,
                known_failure: self.known_failure,
            });
        }

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

            test_file_path.write_str(test_file.contents.as_str())?;

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
            #[cfg(unix)]
            {
                self.run_command_with_pty(test_cmd).await?
            }

            #[cfg(not(unix))]
            {
                panic!("PTY tests are only supported on Unix-like systems");
            }
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
                    (
                        std::process::Command::new(assert_cmd::cargo::cargo_bin(name)),
                        Some(target_dir),
                    )
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

    #[allow(clippy::unused_async)]
    #[cfg(unix)]
    async fn run_command_with_pty(&self, cmd: std::process::Command) -> Result<RunResult> {
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

        if matches!(wait_status, expectrl::WaitStatus::StillAlive) {
            // Try to terminate it safely.
            p.get_process_mut().kill(expectrl::Signal::SIGTERM)?;
            wait_status = p.get_process().wait()?;
        }

        let duration = start_time.elapsed();
        let output = read_expectrl_log(log)?;
        let cleaned = make_expectrl_output_readable(output);

        match wait_status {
            expectrl::WaitStatus::Exited(_, code) => Ok(RunResult {
                exit_status: ExitStatus::from_raw(code),
                stdout: cleaned,
                stderr: String::new(),
                duration,
            }),
            expectrl::WaitStatus::Signaled(_, _, _) => Err(anyhow::anyhow!("process was signaled")),
            _ => Err(anyhow::anyhow!(
                "unexpected status for process: {:?}",
                wait_status
            )),
        }
    }

    #[allow(clippy::unused_async)]
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
            stdout: String::from_utf8(cmd_result.stdout)?,
            stderr: String::from_utf8(cmd_result.stderr)?,
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
    pub fn is_failure(&self) -> bool {
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
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            ExitStatusComparison::TestDiffers {
                test_exit_status: _,
                oracle_exit_status: _
            }
        )
    }
}

#[allow(dead_code)]
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
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            StringComparison::TestDiffers {
                test_string: _,
                oracle_string: _
            }
        )
    }
}

enum DirComparisonEntry {
    LeftOnly(PathBuf),
    RightOnly(PathBuf),
    Different(PathBuf, PathBuf),
}

enum DirComparison {
    Ignored,
    Same,
    TestDiffers(Vec<DirComparisonEntry>),
}

impl DirComparison {
    pub fn is_failure(&self) -> bool {
        matches!(self, DirComparison::TestDiffers(_))
    }
}

fn diff_dirs(oracle_path: &Path, test_path: &Path) -> Result<DirComparison> {
    let profraw_regex = regex::Regex::new(r"\.profraw$")?;
    let filter = dir_cmp::Filter::Exclude(vec![profraw_regex]);

    let options = dir_cmp::Options {
        ignore_equal: true,
        ignore_left_only: false,
        ignore_right_only: false,
        filter: Some(filter),
        recursive: true,
    };

    let result: Vec<_> = dir_cmp::full::compare_dirs(oracle_path, test_path, options)?
        .iter()
        .map(|entry| match entry {
            dir_cmp::full::DirCmpEntry::Left(p) => {
                DirComparisonEntry::LeftOnly(pathdiff::diff_paths(p, oracle_path).unwrap())
            }
            dir_cmp::full::DirCmpEntry::Right(p) => {
                DirComparisonEntry::RightOnly(pathdiff::diff_paths(p, test_path).unwrap())
            }
            dir_cmp::full::DirCmpEntry::Both(l, r, _) => DirComparisonEntry::Different(
                pathdiff::diff_paths(l, oracle_path).unwrap(),
                pathdiff::diff_paths(r, test_path).unwrap(),
            ),
        })
        .collect();

    if result.is_empty() {
        Ok(DirComparison::Same)
    } else {
        Ok(DirComparison::TestDiffers(result))
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
    #[clap(short = 'v', long = "verbose")]
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

    //
    // Compat-only options
    //
    /// Show output from test cases (for compatibility only, has no effect)
    #[clap(long = "show-output")]
    pub show_output: bool,

    /// Capture output? (for compatibility only, has no effect)
    #[clap(long = "nocapture")]
    pub no_capture: bool,

    #[clap(long = "ignored")]
    pub ignored: bool,

    /// Unstable flags (for compatibility only, has no effect)
    #[clap(short = 'Z')]
    pub unstable_flag: Vec<String>,

    //
    // Filters
    //
    pub filters: Vec<String>,
}

impl TestOptions {
    pub fn should_enable_config(&self, config: &str) -> bool {
        let enabled_configs = if self.enabled_configs.is_empty() {
            vec![String::from("bash")]
        } else {
            self.enabled_configs.clone()
        };

        enabled_configs.contains(&config.to_string())
    }

    pub fn should_run_test(&self, test_case_set: &TestCaseSet, test_case: &TestCase) -> bool {
        if self.filters.is_empty() {
            return true;
        }

        let test_case_set_name = test_case_set.name.as_deref().unwrap_or("");
        let test_case_name = test_case.name.as_deref().unwrap_or("");

        if test_case_set_name.is_empty() || test_case_name.is_empty() {
            return false;
        }

        let qualified_name = format!("{test_case_set_name}::{test_case_name}");

        if self.exact_match {
            self.filters.contains(&qualified_name)
        } else {
            self.filters
                .iter()
                .any(|filter| qualified_name.contains(filter))
        }
    }
}

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

fn main() {
    let unparsed_args: Vec<_> = std::env::args().collect();
    let options = TestOptions::parse_from(unparsed_args);

    if options.list_tests_only && options.ignored {
        return;
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(32)
        .build()
        .unwrap()
        .block_on(cli_integration_tests(options))
        .unwrap();
}
