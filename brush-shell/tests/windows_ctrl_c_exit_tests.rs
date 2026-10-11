//! Tests for the exit status of a non-interactive brush that Ctrl-C stopped, on Windows.
//!
//! A child that ends with `STATUS_CONTROL_C_EXIT` stands in for one that Ctrl-C ended; running
//! `cmd /c "exit -1073741510"` produces it without sending a real Ctrl-C.

#![cfg(windows)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;

/// `STATUS_CONTROL_C_EXIT`, as `std::process::ExitStatus::code` reports it.
#[expect(clippy::cast_possible_wrap, reason = "an NTSTATUS is read as an i32")]
const STATUS_CONTROL_C_EXIT: i32 = 0xC000_013A_u32 as i32;

fn cmd_path() -> anyhow::Result<String> {
    std::env::var("ComSpec").context("ComSpec is not set")
}

fn brush() -> std::process::Command {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("brush"));
    cmd.args(["--norc", "--noprofile", "--no-config"]);
    cmd
}

fn script_with(cmd: &str, exit_code: &str) -> String {
    format!("'{cmd}' /c \"exit {exit_code}\"; echo after")
}

#[test]
fn dash_c_stopped_by_ctrl_c_exits_with_ctrl_c_status() -> anyhow::Result<()> {
    let output = brush()
        .arg("-c")
        .arg(script_with(&cmd_path()?, "-1073741510"))
        .output()?;

    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(STATUS_CONTROL_C_EXIT));
    Ok(())
}

#[test]
fn script_stopped_by_ctrl_c_exits_with_ctrl_c_status() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let script_path = dir.path().join("script.sh");
    std::fs::write(&script_path, script_with(&cmd_path()?, "-1073741510"))?;

    let output = brush().arg(&script_path).output()?;

    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(STATUS_CONTROL_C_EXIT));
    Ok(())
}

/// Runs brush with the given script on its standard input, as `brush < script.sh` does.
fn run_from_stdin(script: &str) -> anyhow::Result<std::process::Output> {
    let dir = tempfile::tempdir()?;
    let script_path = dir.path().join("stdin.sh");
    std::fs::write(&script_path, script)?;

    let output = brush().stdin(std::fs::File::open(&script_path)?).output()?;
    Ok(output)
}

/// A script read from standard input runs one line at a time, each with its own execution
/// parameters, so the interruption has to stop the reading of further lines too.
#[test]
fn script_from_stdin_stopped_by_ctrl_c_exits_with_ctrl_c_status() -> anyhow::Result<()> {
    let cmd = cmd_path()?;
    let output = run_from_stdin(&format!("'{cmd}' /c \"exit -1073741510\"\necho after\n"))?;

    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(STATUS_CONTROL_C_EXIT));
    Ok(())
}

#[test]
fn script_from_stdin_after_ordinary_failure_runs_on_and_exits_zero() -> anyhow::Result<()> {
    let cmd = cmd_path()?;
    let output = run_from_stdin(&format!("'{cmd}' /c \"exit 1\"\necho after\n"))?;

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "after");
    assert_eq!(output.status.code(), Some(0));
    Ok(())
}

#[test]
fn dash_c_after_ordinary_failure_runs_on_and_exits_zero() -> anyhow::Result<()> {
    let output = brush()
        .arg("-c")
        .arg(script_with(&cmd_path()?, "1"))
        .output()?;

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "after");
    assert_eq!(output.status.code(), Some(0));
    Ok(())
}
