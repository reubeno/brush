//! Tests for stdin input handling with the minimal input backend.
//!
//! These tests verify multi-line continuation works when reading from stdin.
//! They explicitly use `--input-backend=minimal` to test the minimal backend's
//! handling of incomplete commands that span multiple lines.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use std::io::Write;
use std::process::{Command, Stdio};

struct TestCase {
    name: &'static str,
    input: &'static str,
    expected_words: &'static [&'static str],
}

const CONTINUATION_TESTS: &[TestCase] = &[
    TestCase {
        name: "and_continuation",
        input: "echo one &&\necho two\n",
        expected_words: &["one", "two"],
    },
    TestCase {
        name: "or_continuation",
        input: "false ||\necho fallback\n",
        expected_words: &["fallback"],
    },
    TestCase {
        name: "pipe_then_and_continuation",
        input: "echo one | cat\necho two | cat &&\necho three\n",
        expected_words: &["one", "two", "three"],
    },
];

/// Tests that the minimal input backend correctly handles multi-line continuation.
#[test]
fn multiline_continuation_via_stdin() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    for test in CONTINUATION_TESTS {
        let mut child = Command::new(shell_path)
            .args([
                "--norc",
                "--noprofile",
                "--no-config",
                "--input-backend=minimal",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn brush")?;

        let stdin = child.stdin.as_mut().context("Failed to open stdin")?;
        stdin.write_all(test.input.as_bytes())?;
        drop(child.stdin.take());

        let output = child
            .wait_with_output()
            .context("Failed to wait for brush")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "{}: brush should succeed, stderr: {}",
            test.name,
            stderr
        );

        for word in test.expected_words {
            assert!(
                stdout.contains(word),
                "{}: expected '{}' in output, got: {}",
                test.name,
                word,
                stdout
            );
        }
    }

    Ok(())
}
