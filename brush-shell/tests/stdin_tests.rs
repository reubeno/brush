//! Tests for stdin input handling
//!
//! Tests that verify the shell correctly handles multi-line input when
//! reading from stdin (non-interactive mode).

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use std::io::Write;
use std::process::{Command, Stdio};

/// Test that multi-line input with continuation (e.g., && at end of line) works correctly
/// when piped to the shell. This tests the minimal input backend's handling of incomplete
/// commands that span multiple lines.
#[test]
fn multiline_continuation_via_stdin() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    // Test case: && at end of line requires continuation
    let mut child = Command::new(&shell_path)
        .args(["--norc", "--noprofile", "--no-config"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn brush")?;

    let stdin = child.stdin.as_mut().context("Failed to open stdin")?;
    stdin.write_all(b"echo one &&\necho two\n")?;
    drop(child.stdin.take());

    let output = child.wait_with_output().context("Failed to wait for brush")?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "brush should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("one") && stdout.contains("two"),
        "Expected 'one' and 'two' in output, got: {}",
        stdout
    );

    Ok(())
}

/// Test that more complex multi-line patterns work correctly via stdin
#[test]
fn complex_multiline_via_stdin() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut child = Command::new(&shell_path)
        .args(["--norc", "--noprofile", "--no-config"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn brush")?;

    // Test: pipe followed by newline, then another command with && and newline
    let stdin = child.stdin.as_mut().context("Failed to open stdin")?;
    stdin.write_all(b"echo one | cat\necho two | cat &&\necho three\n")?;
    drop(child.stdin.take());

    let output = child.wait_with_output().context("Failed to wait for brush")?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "brush should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("one") && stdout.contains("two") && stdout.contains("three"),
        "Expected 'one', 'two', and 'three' in output, got: {}",
        stdout
    );

    Ok(())
}

/// Test || continuation works correctly
#[test]
fn or_continuation_via_stdin() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut child = Command::new(&shell_path)
        .args(["--norc", "--noprofile", "--no-config"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn brush")?;

    let stdin = child.stdin.as_mut().context("Failed to open stdin")?;
    stdin.write_all(b"false ||\necho fallback\n")?;
    drop(child.stdin.take());

    let output = child.wait_with_output().context("Failed to wait for brush")?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "brush should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("fallback"),
        "Expected 'fallback' in output, got: {}",
        stdout
    );

    Ok(())
}
