//! Execution logic for running shell commands.

use crate::config::{ShellConfig, WhichShell};
use crate::testcase::{ShellInvocation, TestCase, TestCaseSet, TestFile};
use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::CommandExt, process::ExitStatusExt};
use std::{path::PathBuf, process::ExitStatus};

/// Default timeout for test commands in seconds.
pub const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;

/// Result of running a shell command.
#[derive(Debug)]
pub struct RunResult {
    /// Exit status of the command.
    pub exit_status: ExitStatus,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Duration of the command.
    pub duration: std::time::Duration,
}

impl TestCase {
    /// Runs this test case with the given shell configuration.
    ///
    /// If this test case has a `shell_binary` field set, that binary will be used
    /// instead of the one from `shell_config`.
    pub async fn run_shell(
        &self,
        shell_config: &ShellConfig,
        working_dir: &assert_fs::TempDir,
    ) -> Result<RunResult> {
        // Use per-test shell_binary if specified, otherwise use the config's shell
        let effective_config = if let Some(shell_binary) = &self.shell_binary {
            let binary_path = Self::resolve_shell_binary(shell_binary)?;
            ShellConfig {
                which: WhichShell::ShellUnderTest(binary_path),
                default_args: shell_config.default_args.clone(),
                default_path_var: shell_config.default_path_var.clone(),
            }
        } else {
            shell_config.clone()
        };

        let test_cmd = self.create_command_for_shell(&effective_config, working_dir);

        let result = if self.pty {
            self.run_command_with_pty(test_cmd).await?
        } else {
            self.run_command_with_stdin(test_cmd).await?
        };

        Ok(result)
    }

    /// Resolves a shell binary path.
    ///
    /// If the path is relative, it's resolved against the workspace target directory.
    /// If the path is absolute, it's used as-is.
    fn resolve_shell_binary(shell_binary: &std::path::Path) -> Result<PathBuf> {
        if shell_binary.is_absolute() {
            return Ok(shell_binary.to_path_buf());
        }

        // Resolve relative paths against the target directory
        let cli_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let default_target_dir = || cli_dir.parent().unwrap().join("target");
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .ok()
            .map_or_else(default_target_dir, PathBuf::from);

        // For example paths like "debug/examples/custom-shell" or just "custom-shell"
        // Check target/debug first, then target/release
        let debug_path = target_dir.join("debug").join(shell_binary);
        if debug_path.exists() {
            return Ok(debug_path);
        }

        let release_path = target_dir.join("release").join(shell_binary);
        if release_path.exists() {
            return Ok(release_path);
        }

        // Try directly under target (for profile-qualified paths like "debug/examples/foo")
        let direct_path = target_dir.join(shell_binary);
        if direct_path.exists() {
            return Ok(direct_path);
        }

        Err(anyhow::anyhow!(
            "Could not find shell binary '{}'. Searched:\n  - {}\n  - {}\n  - {}",
            shell_binary.display(),
            debug_path.display(),
            release_path.display(),
            direct_path.display()
        ))
    }

    /// Creates the test files in the given temporary directory.
    pub fn create_test_files_in(
        &self,
        temp_dir: &assert_fs::TempDir,
        test_case_set: &TestCaseSet,
    ) -> Result<()> {
        for test_file in test_case_set
            .common_test_files
            .iter()
            .chain(self.test_files.iter())
        {
            Self::create_test_file(temp_dir, test_file, &test_case_set.source_dir)?;
        }

        Ok(())
    }

    fn create_test_file(
        temp_dir: &assert_fs::TempDir,
        test_file: &TestFile,
        source_dir: &std::path::Path,
    ) -> Result<()> {
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

            let abs_source_path = source_dir.join(source_path);

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

        Ok(())
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

        if matches!(shell_config.which, WhichShell::ShellUnderTest(_)) {
            for arg in &self.additional_test_args {
                test_cmd.arg(arg);
            }
        }

        for arg in &shell_config.default_args {
            if !self.removed_default_args.contains(arg) {
                test_cmd.arg(arg);
            }
        }

        // Clear all environment vars for consistency.
        test_cmd.args(&self.args).env_clear();

        // Hard-code a well known prompt for PS1.
        test_cmd.env("PS1", "test$ ");
        // Try to get decent backtraces when problems get hit.
        test_cmd.env("RUST_BACKTRACE", "1");
        // Compute a PATH that contains what we need.
        test_cmd.env("PATH", shell_config.compute_test_path_var());

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

        if let Some(home_dir) = &self.home_dir {
            let abs_home_dir = if home_dir.is_relative() {
                working_dir.join(home_dir)
            } else {
                home_dir.to_owned()
            };

            test_cmd.env("HOME", abs_home_dir.to_string_lossy().to_string());
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
        use crate::util::{make_expectrl_output_readable, read_expectrl_log};
        use expectrl::{Expect, process::Termios as _};

        let mut log = Vec::new();
        let writer = std::io::Cursor::new(&mut log);

        let start_time = std::time::Instant::now();
        let mut p = expectrl::session::log(expectrl::Session::spawn(cmd)?, writer)?;
        p.set_echo(true)?;

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
    #[allow(unused_mut, reason = "only mutated on some platforms")]
    async fn run_command_with_stdin(&self, mut cmd: std::process::Command) -> Result<RunResult> {
        // SAFETY:
        // To avoid bash trying to directly access /dev/tty and generate tty-related signals,
        // we create a new session for the child process. The standard library has a setsid()
        // API but it's unstable, so we use nix here. Calling pre_exec can be unsafe as
        // it runs in the child process after fork() but before exec(), and there are constraints
        // around what can be safely done in that context. However, calling setsid() is generally
        // considered safe as it doesn't allocate memory or perform complex operations to forked
        // state.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                let _ = nix::unistd::setsid();
                Ok(())
            })
        };

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
}
