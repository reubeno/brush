use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
use colored::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, process::ExitStatus};

#[test]
fn cli_integration_tests() -> Result<()> {
    let dir = env!("CARGO_MANIFEST_DIR");

    let mut success_count = 0;
    let mut expected_fail_count = 0;
    let mut fail_count = 0;
    for entry in glob::glob(format!("{}/tests/cases/**/*.yaml", dir).as_ref()).unwrap() {
        let entry = entry.unwrap();

        let yaml_file = std::fs::File::open(entry.as_path())?;
        let test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)
            .context(format!("parsing {}", entry.to_string_lossy()))?;

        let results = test_case_set.run()?;

        success_count += results.success_count;
        expected_fail_count += results.expected_fail_count;
        fail_count += results.fail_count;
    }

    let formatted_fail_count = if fail_count > 0 {
        fail_count.to_string().red()
    } else {
        fail_count.to_string().green()
    };

    let formatted_expected_fail_count = if expected_fail_count > 0 {
        expected_fail_count.to_string().magenta()
    } else {
        expected_fail_count.to_string().green()
    };

    println!("==============================================================");
    println!(
        "{} test case(s) ran: {} succeeded, {} failed, {} expected to fail.",
        success_count + fail_count,
        success_count.to_string().green(),
        formatted_fail_count,
        formatted_expected_fail_count
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

struct TestCaseSetResults {
    pub success_count: u32,
    pub expected_fail_count: u32,
    pub fail_count: u32,
}

impl TestCaseSet {
    pub fn run(&self) -> Result<TestCaseSetResults> {
        println!(
            "=================== {}: [{}] ===================",
            "Running test case set".blue(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        let mut success_count = 0;
        let mut expected_fail_count = 0;
        let mut fail_count = 0;
        for test_case in self.cases.iter() {
            if test_case.run()? {
                if test_case.expected_failure {
                    fail_count += 1;
                } else {
                    success_count += 1;
                }
            } else if test_case.expected_failure {
                expected_fail_count += 1;
            } else {
                fail_count += 1;
            }
        }

        Ok(TestCaseSetResults {
            success_count,
            expected_fail_count,
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
    pub expected_failure: bool,
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

impl TestCase {
    pub fn run(&self) -> Result<bool> {
        print!(
            "* {}: [{}]... ",
            "Running test case".bright_yellow(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        let comparison = self.run_with_oracle_and_test()?;

        let success = !comparison.is_failure();
        if success {
            if self.expected_failure {
                println!("{}", "unexpected success.".bright_red());
            } else {
                println!("{}", "ok.".bright_green());
                return Ok(true);
            }
        } else if self.expected_failure {
            println!("{}", "expected failure.".bright_magenta());
            return Ok(false);
        }

        println!();

        match comparison.exit_status {
            ExitStatusComparison::Ignored => println!("    status {}", "ignored".cyan()),
            ExitStatusComparison::Same(status) => {
                println!(
                    "    status matches ({}) {}",
                    format!("{status}").green(),
                    "✔️".green()
                )
            }
            ExitStatusComparison::TestDiffers {
                test_exit_status,
                oracle_exit_status,
            } => {
                println!(
                    "    status mismatch: {} from oracle vs. {} from test",
                    format!("{}", oracle_exit_status).cyan(),
                    format!("{}", test_exit_status).bright_red()
                );
            }
        }

        match comparison.stdout {
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

        match comparison.stderr {
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

        match comparison.temp_dir {
            DirComparison::Ignored => println!("    temp dir {}", "ignored".cyan()),
            DirComparison::Same => println!("    temp dir matches {}", "✔️".green()),
            DirComparison::TestDiffers => println!("    temp dir {}", "DIFFERS".bright_red()),
        }

        if !success {
            println!("    {}", "FAILED.".bright_red());
        }

        Ok(success)
    }

    fn create_test_files_in(&self, temp_dir: &assert_fs::TempDir) -> Result<()> {
        for test_file in &self.test_files {
            temp_dir
                .child(test_file.path.as_path())
                .write_str(test_file.contents.as_str())?;
        }

        Ok(())
    }

    fn run_with_oracle_and_test(&self) -> Result<RunComparison> {
        let oracle_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&oracle_temp_dir)?;
        let oracle_result =
            self.run_with_shell(&WhichShell::NamedShell("bash".to_owned()), &oracle_temp_dir)?;

        let test_temp_dir = assert_fs::TempDir::new()?;
        self.create_test_files_in(&test_temp_dir)?;
        let test_result = self.run_with_shell(
            &WhichShell::ShellUnderTest("rush".to_owned()),
            &test_temp_dir,
        )?;

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
        let temp_dir_diff =
            dir_diff::is_different(oracle_temp_dir.path(), test_temp_dir.path()).unwrap();
        if !temp_dir_diff {
            comparison.temp_dir = DirComparison::Same;
        } else {
            comparison.temp_dir = DirComparison::TestDiffers;
        }

        Ok(comparison)
    }

    fn run_with_shell(
        &self,
        which: &WhichShell,
        working_dir: &assert_fs::TempDir,
    ) -> Result<RunResult> {
        let mut test_cmd = match self.invocation {
            ShellInvocation::ExecShellBinary => match which {
                WhichShell::ShellUnderTest(name) => assert_cmd::Command::cargo_bin(name)?,
                WhichShell::NamedShell(name) => assert_cmd::Command::new(name),
            },
            ShellInvocation::ExecScript(_) => todo!("exec script test"),
        };

        // TODO: Find a better place for these.
        test_cmd.arg("--norc");
        test_cmd.arg("--noprofile");

        test_cmd.args(&self.args).env_clear();

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

enum DirComparison {
    Ignored,
    Same,
    TestDiffers,
}

impl DirComparison {
    pub fn is_failure(&self) -> bool {
        matches!(self, DirComparison::TestDiffers)
    }
}
