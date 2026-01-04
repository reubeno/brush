//! Completion integration tests for brush shell.

// For now, only compile this for Linux.
#![cfg(target_os = "linux")]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use assert_fs::prelude::*;
use brush_builtins::ShellBuilderExt;
use brush_core::ShellRuntime as _;
use std::path::PathBuf;

struct TestShellWithBashCompletion {
    shell: brush_core::Shell,
    temp_dir: assert_fs::TempDir,
}

const DEFAULT_BASH_COMPLETION_SCRIPT: &str = "/usr/share/bash-completion/bash_completion";

impl TestShellWithBashCompletion {
    async fn new() -> Result<Self> {
        let mut shell = brush_core::Shell::builder()
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .default_builtins(brush_builtins::BuiltinSet::BashMode)
            .build()
            .await?;

        let temp_dir = assert_fs::TempDir::new()?;
        let bash_completion_script_path = Self::find_bash_completion_script()?;

        let exec_params = brush_core::ExecutionParameters::default();
        let source_result = shell
            .source_script(
                bash_completion_script_path.as_path(),
                std::iter::empty::<String>(),
                &exec_params,
            )
            .await?;

        if !source_result.is_success() {
            return Err(anyhow::anyhow!("failed to source bash completion script"));
        }

        shell.set_working_dir(temp_dir.path())?;

        Ok(Self { shell, temp_dir })
    }

    fn find_bash_completion_script() -> Result<PathBuf> {
        // See if an environmental override was provided.
        let script_path = std::env::var("BASH_COMPLETION_PATH").map_or_else(
            |_| PathBuf::from(DEFAULT_BASH_COMPLETION_SCRIPT),
            PathBuf::from,
        );

        if script_path.exists() {
            Ok(script_path)
        } else {
            Err(anyhow::anyhow!(
                "bash completion script not found: {}",
                script_path.display()
            ))
        }
    }

    pub async fn complete_end_of_line(&mut self, line: &str) -> Result<Vec<String>> {
        self.complete(line, line.len()).await
    }

    pub async fn complete(&mut self, line: &str, pos: usize) -> Result<Vec<String>> {
        let completions = self.shell.complete(line, pos).await?;
        Ok(completions.candidates.into_iter().collect())
    }

    pub fn set_var(&mut self, name: &str, value: &str) -> Result<()> {
        self.shell
            .env_mut()
            .set_global(name, brush_core::ShellVariable::new(value))?;
        Ok(())
    }
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_file_path() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, ["item1", "item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_file_path_ignoring_case() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;
    test_shell
        .shell
        .options_mut()
        .case_insensitive_pathname_expansion = true;

    // Create file and dir.
    test_shell.temp_dir.child("ITEM1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, ["ITEM1", "item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_relative_dir_path() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see just the dir.
    let results = test_shell.complete_end_of_line("cd item").await?;

    assert_eq!(results, ["item2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_under_empty_dir() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("empty").create_dir_all()?;

    // Complete; expect to see nothing.
    let results = test_shell.complete_end_of_line("ls empty/").await?;

    assert_eq!(results, Vec::<String>::new());

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_nonexistent_relative_path() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete; expect to see nothing.
    let results = test_shell.complete_end_of_line("ls item").await?;

    assert_eq!(results, Vec::<String>::new());

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_absolute_paths() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see just the dir.
    let input = std::format!("ls {}", test_shell.temp_dir.path().join("item").display());
    let results = test_shell.complete_end_of_line(input.as_str()).await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_with_var() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls $PWD/item").await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_with_tilde() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Set HOME to the temp dir so we can use ~ to reference it.
    test_shell.set_var(
        "HOME",
        test_shell
            .temp_dir
            .path()
            .to_string_lossy()
            .to_string()
            .as_str(),
    )?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let results = test_shell.complete_end_of_line("ls ~/item").await?;

    assert_eq!(
        results,
        [
            test_shell
                .temp_dir
                .child("item1")
                .path()
                .display()
                .to_string(),
            test_shell
                .temp_dir
                .child("item2")
                .path()
                .display()
                .to_string(),
        ]
    );

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_variable_names() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let results = test_shell.complete_end_of_line("echo $TESTVAR").await?;
    assert_eq!(results, ["$TESTVAR1", "$TESTVAR2"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_variable_names_with_braces() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let results = test_shell.complete_end_of_line("echo ${TESTVAR").await?;
    assert_eq!(results, ["${TESTVAR1}", "${TESTVAR2}"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_help_topic() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("help expor").await?;
    assert_eq!(results, ["export"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_command_option() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("ls --hel").await?;
    assert_eq!(results, ["--help"]);

    Ok(())
}

/// Tests completion with some well-known programs that have been good manual test cases
/// for us in the past.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_path_args_to_well_known_programs() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete.
    let results = test_shell.complete_end_of_line("tar tvf ./item").await?;

    assert_eq!(results, ["./item2"]);

    Ok(())
}

/// Tests some 'find' completion.
#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_find_command() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete.
    let results = test_shell.complete_end_of_line("find . -na").await?;

    assert_eq!(results, ["-name"]);

    Ok(())
}

#[test_with::file(/usr/share/bash-completion/bash_completion)]
#[tokio::test(flavor = "multi_thread")]
async fn complete_quoted_filenames() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    test_shell.temp_dir.child("item1 item2").touch()?;
    test_shell.temp_dir.child("item1'item2").touch()?;

    let mut results = test_shell.complete_end_of_line("ls item1\\ ").await?;
    assert_eq!(results, ["item1 item2"]);

    results = test_shell.complete_end_of_line("ls item1'").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    results = test_shell.complete_end_of_line("ls item1").await?;
    assert_eq!(results, ["item1 item2", "item1'item2"]);

    results = test_shell.complete_end_of_line("ls 'item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    results = test_shell.complete_end_of_line("ls \"item1 ").await?;
    assert_eq!(results, ["item1 item2"]);

    Ok(())
}
