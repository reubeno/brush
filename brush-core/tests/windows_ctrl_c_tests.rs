//! Tests for how the shell treats an external command that Ctrl-C ended on Windows.
//!
//! A process that Windows ends because of Ctrl-C exits with `STATUS_CONTROL_C_EXIT`
//! (`0xC000013A`). The tests stand in for a real Ctrl-C with `cmd /c "exit -1073741510"`,
//! which exits with that same status, so no console event has to be sent.

#![cfg(windows)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;

/// `STATUS_CONTROL_C_EXIT` as the signed value that `cmd`'s `exit` takes.
const CTRL_C_EXIT: &str = "-1073741510";

async fn new_shell() -> Result<brush_core::Shell> {
    Ok(brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?)
}

/// Returns a command line that runs `cmd` by absolute path, so it doesn't depend on `PATH`.
fn cmd_exit(code: &str) -> String {
    let comspec =
        std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into());
    std::format!(r#"'{comspec}' /c "exit {code}""#)
}

async fn run(shell: &mut brush_core::Shell, script: &str) -> Result<()> {
    let params = shell.default_exec_params();
    shell
        .run_string(script, &brush_core::SourceInfo::default(), &params)
        .await?;
    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_sets_status_to_130() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &cmd_exit(CTRL_C_EXIT)).await?;

    // bash reports a command that SIGINT ended as 128 + 2. Without the translation the
    // status would be the low 8 bits of 0xC000013A, which is 58.
    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ordinary_failure_keeps_its_status() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &cmd_exit("1")).await?;

    assert_eq!(shell.last_exit_status(), 1);

    Ok(())
}
