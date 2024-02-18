use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
use colored::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::ExitStatus,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 20)]
async fn cli_integration_tests() -> Result<()> {
    let dir = env!("CARGO_MANIFEST_DIR");

    let mut success_count = 0;
    let mut known_failure_count = 0;
    let mut fail_count = 0;
    let mut join_handles = vec![];

    // Spawn each test case set separately.
    for entry in glob::glob(format!("{dir}/tests/cases/**/*.yaml").as_ref()).unwrap() {
        let entry = entry.unwrap();

        let yaml_file = std::fs::File::open(entry.as_path())?;
        let test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)
            .context(format!("parsing {}", entry.to_string_lossy()))?;

        join_handles.push(tokio::spawn(async move { test_case_set.run().await }));
    }

    // Now go through and await everything.
    for join_handle in join_handles {
        let results = join_handle.await??;

        success_count += results.success_count;
        known_failure_count += results.known_failure_count;
        fail_count += results.fail_count;

        results.report();
    }

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

    println!("==============================================================");
    println!(
        "{} test case(s) ran: {} succeeded, {} failed, {} known to fail.",
        success_count + fail_count + known_failure_count,
        success_count.to_string().green(),
        formatted_fail_count,
        formatted_known_failure_count
    );
    println!("==============================================================");

    assert!(fail_count == 0);

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct TestCaseSet {
    /// Name of the test case set
    pub name: Option<String>,
    /// Set of test cases
    pub cases: Vec<TestCase>,
}

#[allow(clippy::struct_field_names)]
struct TestCaseSetResults {
    pub name: Option<String>,
    pub success_count: u32,
    pub known_failure_count: u32,
    pub fail_count: u32,
    pub test_case_results: Vec<TestCaseResult>,
}

impl TestCaseSetResults {
    pub fn report(&self) {
        println!(
            "=================== {}: [{}] ===================",
            "Running test case set".blue(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        for test_case_result in &self.test_case_results {
            test_case_result.report();
        }
    }
}

impl TestCaseSet {
    pub async fn run(&self) -> Result<TestCaseSetResults> {
        let mut success_count = 0;
        let mut known_failure_count = 0;
        let mut fail_count = 0;
        let mut test_case_results = vec![];
        for test_case in &self.cases {
            let test_case_result = test_case.run().await?;

            if test_case_result.success {
                if test_case.known_failure {
                    fail_count += 1;
                } else {
                    success_count += 1;
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
            test_case_results,
            success_count,
            known_failure_count,
            fail_count,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
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
    pub stdin: Option<String>,
    #[serde(default)]
    pub ignore_exit_status: bool,
    #[serde(default)]
    pub ignore_stderr: bool,
    #[serde(default)]
    pub ignore_stdout: bool,
    #[serde(default)]
    pub test_files: Vec<TestFile>,
    #[serde(default)]
    pub known_failure: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct TestFile {
    /// Relative path to test file
    pub path: PathBuf,
    /// Contents to seed the file with
    pub contents: String,
}

#[derive(Debug, Deserialize, Serialize)]
enum ShellInvocation {
    ExecShellBinary,
    ExecScript(String),
}

impl Default for ShellInvocation {
    fn default() -> Self {
        Self::ExecShellBinary
    }
}

enum WhichShell {
    ShellUnderTest(String),
    NamedShell(String),
}

struct TestCaseResult {
    pub name: Option<String>,
    pub success: bool,
    pub known_failure: bool,
    pub comparison: RunComparison,
}

impl TestCaseResult {
    #[allow(clippy::too_many_lines)]
    pub fn report(&self) {
        print!(
            "* {}: [{}]... ",
            "Test case".bright_yellow(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        if !self.comparison.is_failure() {
            if self.known_failure {
                println!("{}", "unexpected success.".bright_red());
            } else {
                println!("{}", "ok.".bright_green());
                return;
            }
        } else if self.known_failure {
            println!("{}", "known failure.".bright_magenta());
            return;
        }

        println!();

        match self.comparison.exit_status {
            ExitStatusComparison::Ignored => println!("    status {}", "ignored".cyan()),
            ExitStatusComparison::Same(status) => {
                println!(
                    "    status matches ({}) {}",
                    format!("{status}").green(),
                    "✔️".green()
                );
            }
            ExitStatusComparison::TestDiffers {
                test_exit_status,
                oracle_exit_status,
            } => {
                println!(
                    "    status mismatch: {} from oracle vs. {} from test",
                    format!("{oracle_exit_status}").cyan(),
                    format!("{test_exit_status}").bright_red()
                );
            }
        }

        match &self.comparison.stdout {
            StringComparison::Ignored => println!("    stdout {}", "ignored".cyan()),
            StringComparison::Same(_) => println!("    stdout matches {}", "✔️".green()),
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                println!("    stdout {}", "DIFFERS:".bright_red());

                println!(
                    "        {}",
                    "------ Oracle <> Test: stdout ---------------------------------".cyan()
                );

                println!(
                    "{}",
                    indent::indent_all_by(
                        8,
                        prettydiff::diff_lines(o.as_str(), t.as_str()).format()
                    )
                );

                println!(
                    "        {}",
                    "---------------------------------------------------------------".cyan()
                );
            }
        }

        match &self.comparison.stderr {
            StringComparison::Ignored => println!("    stderr {}", "ignored".cyan()),
            StringComparison::Same(_) => println!("    stderr matches {}", "✔️".green()),
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                println!("    stderr {}", "DIFFERS:".bright_red());

                println!(
                    "        {}",
                    "------ Oracle <> Test: stderr ---------------------------------".cyan()
                );

                println!(
                    "{}",
                    indent::indent_all_by(
                        8,
                        prettydiff::diff_lines(o.as_str(), t.as_str()).format()
                    )
                );

                println!(
                    "        {}",
                    "---------------------------------------------------------------".cyan()
                );
            }
        }

        match &self.comparison.temp_dir {
            DirComparison::Ignored => println!("    temp dir {}", "ignored".cyan()),
            DirComparison::Same => println!("    temp dir matches {}", "✔️".green()),
            DirComparison::TestDiffers(entries) => {
                println!("    temp dir {}", "DIFFERS".bright_red());

                for entry in entries {
                    const INDENT: &str = "        ";
                    match entry {
                        DirComparisonEntry::Different(left, right) => {
                            println!(
                                "{INDENT}oracle file {} differs from test file {}",
                                left.to_string_lossy(),
                                right.to_string_lossy()
                            );
                        }
                        DirComparisonEntry::LeftOnly(p) => {
                            println!(
                                "{INDENT}file missing from test dir: {}",
                                p.to_string_lossy()
                            );
                        }
                        DirComparisonEntry::RightOnly(p) => {
                            println!(
                                "{INDENT}unexpected file in test dir: {}",
                                p.to_string_lossy()
                            );
                        }
                    }
                }
            }
        }

        if !self.success {
            println!("    {}", "FAILED.".bright_red());
        }
    }
}

impl TestCase {
    pub async fn run(&self) -> Result<TestCaseResult> {
        let comparison = self.run_with_oracle_and_test().await?;
        let success = !comparison.is_failure() && !self.known_failure;
        Ok(TestCaseResult {
            success,
            comparison,
            name: self.name.clone(),
            known_failure: self.known_failure,
        })
    }

    fn create_test_files_in(&self, temp_dir: &assert_fs::TempDir) -> Result<()> {
        for test_file in &self.test_files {
            temp_dir
                .child(test_file.path.as_path())
                .write_str(test_file.contents.as_str())?;
        }

        Ok(())
    }

    async fn run_with_oracle_and_test(&self) -> Result<RunComparison> {
        let oracle_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&oracle_temp_dir)?;
        let oracle_result = self
            .run_with_shell(&WhichShell::NamedShell("bash".to_owned()), &oracle_temp_dir)
            .await?;

        let test_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&test_temp_dir)?;
        let test_result = self
            .run_with_shell(
                &WhichShell::ShellUnderTest("brush".to_owned()),
                &test_temp_dir,
            )
            .await?;

        let mut comparison = RunComparison {
            exit_status: ExitStatusComparison::Ignored,
            stdout: StringComparison::Ignored,
            stderr: StringComparison::Ignored,
            temp_dir: DirComparison::Ignored,
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
            comparison.stdout = StringComparison::Ignored;
        } else if oracle_result.stdout == test_result.stdout {
            comparison.stdout = StringComparison::Same(oracle_result.stdout);
        } else {
            comparison.stdout = StringComparison::TestDiffers {
                test_string: test_result.stdout,
                oracle_string: oracle_result.stdout,
            }
        }

        // Compare stderr
        if self.ignore_stderr {
            comparison.stderr = StringComparison::Ignored;
        } else if oracle_result.stderr == test_result.stderr {
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

    #[allow(clippy::unused_async)]
    async fn run_with_shell(
        &self,
        which: &WhichShell,
        working_dir: &assert_fs::TempDir,
    ) -> Result<RunResult> {
        let (mut test_cmd, coverage_target_dir) = match self.invocation {
            ShellInvocation::ExecShellBinary => match which {
                WhichShell::ShellUnderTest(name) => {
                    let cli_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                    let target_dir = cli_dir.parent().unwrap().join("target");
                    (assert_cmd::Command::cargo_bin(name)?, Some(target_dir))
                }
                WhichShell::NamedShell(name) => (assert_cmd::Command::new(name), None),
            },
            ShellInvocation::ExecScript(_) => todo!("UNIMPLEMENTED: exec script test"),
        };

        // Skip rc file and profile for deterministic behavior across systems/distros.
        test_cmd.arg("--norc");
        test_cmd.arg("--noprofile");

        // Clear all environment vars for consistency.
        test_cmd.args(&self.args).env_clear();

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

        if let Some(stdin) = &self.stdin {
            test_cmd.write_stdin(stdin.as_bytes());
        }

        let cmd_result = test_cmd.output()?;

        Ok(RunResult {
            exit_status: cmd_result.status,
            stdout: String::from_utf8(cmd_result.stdout)?,
            stderr: String::from_utf8(cmd_result.stderr)?,
        })
    }
}

struct RunResult {
    pub exit_status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

struct RunComparison {
    pub exit_status: ExitStatusComparison,
    pub stdout: StringComparison,
    pub stderr: StringComparison,
    pub temp_dir: DirComparison,
}

impl RunComparison {
    pub fn is_failure(&self) -> bool {
        self.exit_status.is_failure()
            || self.stdout.is_failure()
            || self.stderr.is_failure()
            || self.temp_dir.is_failure()
    }
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

enum StringComparison {
    Ignored,
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
