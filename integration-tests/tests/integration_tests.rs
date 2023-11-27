use anyhow::Result;
use colored::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, process::ExitStatus};

#[test]
fn cli_tests() -> Result<()> {
    let dir = env!("CARGO_MANIFEST_DIR");
    for entry in glob::glob(format!("{}/tests/cases/**/*.yaml", dir).as_ref()).unwrap() {
        let entry = entry.unwrap();

        let yaml_file = std::fs::File::open(entry)?;
        let test_case_set: TestCaseSet = serde_yaml::from_reader(yaml_file)?;

        test_case_set.run()?;
    }

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct TestCaseSet {
    /// Name of the test case set
    pub name: Option<String>,
    /// Set of test cases
    pub cases: Vec<TestCase>,
}

impl TestCaseSet {
    pub fn run(&self) -> Result<()> {
        println!(
            "{}: [{}]...",
            "Running test case set".blue(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        let mut success = true;
        let mut cases_succeeded = 0;

        for test_case in self.cases.iter() {
            let current_success = test_case.run()?;
            if current_success {
                cases_succeeded += 1;
            }

            success = success && current_success;
        }

        println!("==============================================================");
        println!(
            "{} test case(s) ran: {} succeeded, {} failed.",
            self.cases.len(),
            cases_succeeded.to_string().green(),
            (self.cases.len() - cases_succeeded).to_string().red(),
        );
        println!("==============================================================");

        assert!(success);

        Ok(())
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
        println!(
            "{}: [{}]... ",
            "Running test case".bright_yellow(),
            self.name
                .as_ref()
                .unwrap_or(&("(unnamed)".to_owned()))
                .italic()
        );

        let comparison = self.run_with_oracle_and_test()?;

        let mut success = true;

        match comparison.exit_status {
            ExitStatusComparison::Ignored => println!("    status {}", "ignored".cyan()),
            ExitStatusComparison::Same(status) => {
                println!(
                    "    status matches ({}) {}",
                    format!("{}", status).green(),
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
                success = false;
            }
        }

        match comparison.stdout {
            StringComparison::Ignored => println!("    stdout {}", "ignored".cyan()),
            StringComparison::Same(_) => println!("    stdout matches {}", "✔️".green()),
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                println!("    Stdout {}", "DIFFERS".bright_red());

                println!(
                    "{}",
                    "------ Oracle's stdout ---------------------------------".cyan()
                );
                println!("{}", o);
                println!(
                    "{}",
                    "------ Test's stdout -----------------------------------".cyan()
                );
                println!("{}", t);
                println!(
                    "{}",
                    "--------------------------------------------------------".cyan()
                );

                success = false;
            }
        }

        match comparison.stderr {
            StringComparison::Ignored => println!("    stderr {}", "ignored".cyan()),
            StringComparison::Same(_) => println!("    stderr matches {}", "✔️".green()),
            StringComparison::TestDiffers {
                test_string: t,
                oracle_string: o,
            } => {
                println!("    stderr {}", "DIFFERS".bright_red());

                println!(
                    "{}",
                    "------ Oracle's stderr ---------------------------------".cyan()
                );
                println!("{}", o);
                println!(
                    "{}",
                    "------ Test's stderr -----------------------------------".cyan()
                );
                println!("{}", t);
                println!(
                    "{}",
                    "--------------------------------------------------------".cyan()
                );

                success = false;
            }
        }

        match comparison.temp_dir {
            DirComparison::Ignored => println!("    temp dir {}", "ignored".cyan()),
            DirComparison::Same => println!("    temp dir matches {}", "✔️".green()),
            DirComparison::TestDiffers => println!("    temp dir {}", "DIFFERS".bright_red()),
        }

        if success {
            println!("    {}", "ok.".bright_green());
        } else {
            println!("    {}", "FAILED.".bright_red());
        }

        Ok(success)
    }

    fn run_with_oracle_and_test(&self) -> Result<RunComparison> {
        let oracle_temp_dir = assert_fs::TempDir::new()?;
        let oracle_result =
            self.run_with_shell(&WhichShell::NamedShell("bash".to_owned()), &oracle_temp_dir)?;

        let test_temp_dir = assert_fs::TempDir::new()?;
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
        if oracle_result.exit_status == test_result.exit_status {
            comparison.exit_status = ExitStatusComparison::Same(oracle_result.exit_status);
        } else {
            comparison.exit_status = ExitStatusComparison::TestDiffers {
                test_exit_status: test_result.exit_status,
                oracle_exit_status: oracle_result.exit_status,
            }
        }

        // Compare stdout
        if oracle_result.stdout == test_result.stdout {
            comparison.stdout = StringComparison::Same(oracle_result.stdout);
        } else {
            comparison.stdout = StringComparison::TestDiffers {
                test_string: test_result.stdout,
                oracle_string: oracle_result.stdout,
            }
        }

        // Compare stderr
        if oracle_result.stderr == test_result.stderr {
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

enum ExitStatusComparison {
    Ignored,
    Same(ExitStatus),
    TestDiffers {
        test_exit_status: ExitStatus,
        oracle_exit_status: ExitStatus,
    },
}

enum StringComparison {
    Ignored,
    Same(String),
    TestDiffers {
        test_string: String,
        oracle_string: String,
    },
}

enum DirComparison {
    Ignored,
    Same,
    TestDiffers,
}
