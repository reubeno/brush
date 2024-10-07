// For now, only compile this for Linux.
#![cfg(target_os = "linux")]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use assert_fs::prelude::*;
use std::path::Path;

struct TestShellWithBashCompletion {
    shell: brush_core::Shell,
    temp_dir: assert_fs::TempDir,
}

const BASH_COMPLETION_SCRIPT: &str = "/usr/share/bash-completion/bash_completion";

impl TestShellWithBashCompletion {
    async fn new() -> Result<Self> {
        let temp_dir = assert_fs::TempDir::new()?;

        let create_options = brush_core::CreateOptions {
            no_profile: true,
            no_rc: true,
            ..Default::default()
        };

        let mut shell = brush_core::Shell::new(&create_options).await?;

        let exec_params = shell.default_exec_params();
        let source_result = shell
            .source::<String>(Path::new(BASH_COMPLETION_SCRIPT), &[], &exec_params)
            .await?;

        if source_result.exit_code != 0 {
            return Err(anyhow::anyhow!("failed to source bash completion script"));
        }

        shell.set_working_dir(temp_dir.path())?;

        Ok(Self { shell, temp_dir })
    }

    pub async fn complete(&mut self, line: &str, pos: usize) -> Result<Vec<String>> {
        let completions = self.shell.get_completions(line, pos).await?;
        Ok(completions.candidates.into_iter().collect())
    }

    pub fn set_var(&mut self, name: &str, value: &str) -> Result<()> {
        self.shell
            .env
            .set_global(name, brush_core::ShellVariable::new(value.into()))?;
        Ok(())
    }
}

#[tokio::test]
async fn complete_relative_file_path() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see the two files.
    let input = "ls item";
    let results = test_shell.complete(input, input.len()).await?;

    assert_eq!(results, ["item1", "item2"]);

    Ok(())
}

#[tokio::test]
async fn complete_relative_dir_path() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Create file and dir.
    test_shell.temp_dir.child("item1").touch()?;
    test_shell.temp_dir.child("item2").create_dir_all()?;

    // Complete; expect to see just the dir.
    let input = "cd item";
    let results = test_shell.complete(input, input.len()).await?;

    assert_eq!(results, ["item2"]);

    Ok(())
}

#[tokio::test]
async fn complete_variable_names() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let input = "echo $TESTVAR";
    let results = test_shell.complete(input, input.len()).await?;
    assert_eq!(results, ["$TESTVAR1", "$TESTVAR2"]);

    Ok(())
}

#[tokio::test]
async fn complete_variable_names_with_braces() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Set a few vars.
    test_shell.set_var("TESTVAR1", "")?;
    test_shell.set_var("TESTVAR2", "")?;

    // Complete.
    let input = "echo ${TESTVAR";
    let results = test_shell.complete(input, input.len()).await?;
    assert_eq!(results, ["${TESTVAR1}", "${TESTVAR2}"]);

    Ok(())
}

#[tokio::test]
async fn complete_help_topic() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete.
    let input = "help expor";
    let results = test_shell.complete(input, input.len()).await?;
    assert_eq!(results, ["export"]);

    Ok(())
}

#[tokio::test]
async fn complete_command_option() -> Result<()> {
    let mut test_shell = TestShellWithBashCompletion::new().await?;

    // Complete.
    let input = "ls --hel";
    let results = test_shell.complete(input, input.len()).await?;
    assert_eq!(results, ["--help"]);

    Ok(())
}
