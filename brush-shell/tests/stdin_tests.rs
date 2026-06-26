//! Tests for the minimal input backend's multi-line continuation handling.
//!
//! These tests verify that `MinimalInputBackend::read_line_from` correctly
//! accumulates lines when the parser indicates incomplete input.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::panic)]

use std::io::Cursor;
use std::sync::Arc;

use brush_interactive::{InteractivePrompt, MinimalInputBackend, ReadResult};
use tokio::sync::Mutex;

struct TestCase {
    name: &'static str,
    input: &'static str,
    expected: &'static str,
}

const CONTINUATION_TESTS: &[TestCase] = &[
    TestCase {
        name: "and_continuation",
        input: "echo one &&\necho two\n",
        expected: "echo one &&\necho two\n",
    },
    TestCase {
        name: "or_continuation",
        input: "false ||\necho fallback\n",
        expected: "false ||\necho fallback\n",
    },
    TestCase {
        name: "pipe_then_and_continuation",
        input: "echo one | cat &&\necho two\n",
        expected: "echo one | cat &&\necho two\n",
    },
];

async fn create_test_shell() -> brush_interactive::ShellRef {
    let shell = brush_core::Shell::builder().build().await.unwrap();
    Arc::new(Mutex::new(shell))
}

fn create_prompt() -> InteractivePrompt {
    InteractivePrompt {
        prompt: String::from("$ "),
        alt_side_prompt: String::new(),
        continuation_prompt: String::from("> "),
    }
}

/// Tests that the minimal input backend correctly accumulates lines for
/// incomplete commands (those ending with && or ||).
#[tokio::test(flavor = "multi_thread")]
async fn multiline_continuation() {
    let shell_ref = create_test_shell().await;
    let prompt = create_prompt();

    for test in CONTINUATION_TESTS {
        let mut backend = MinimalInputBackend;
        let mut reader = Cursor::new(test.input.as_bytes());

        let result = tokio::task::block_in_place(|| {
            backend.read_line_from(&shell_ref, &prompt, &mut reader, false)
        });

        let name = test.name;
        match result {
            Ok(ReadResult::Input(input)) => {
                assert_eq!(input, test.expected, "{name}: unexpected input");
            }
            Ok(ReadResult::Eof) => {
                panic!("{name}: expected Input, got Eof");
            }
            Ok(ReadResult::Interrupted) => {
                panic!("{name}: expected Input, got Interrupted");
            }
            Ok(ReadResult::BoundCommand(cmd)) => {
                panic!("{name}: expected Input, got BoundCommand({cmd})");
            }
            Err(e) => {
                panic!("{name}: unexpected error: {e}");
            }
        }
    }
}

/// Tests that complete single-line commands are returned immediately.
#[tokio::test(flavor = "multi_thread")]
async fn complete_single_line() {
    let shell_ref = create_test_shell().await;
    let prompt = create_prompt();
    let mut backend = MinimalInputBackend;
    let mut reader = Cursor::new(b"echo hello\n" as &[u8]);

    let result = tokio::task::block_in_place(|| {
        backend.read_line_from(&shell_ref, &prompt, &mut reader, false)
    });

    match result {
        Ok(ReadResult::Input(input)) => {
            assert_eq!(input, "echo hello\n");
        }
        Ok(_) => {
            panic!("expected Input, got other ReadResult variant");
        }
        Err(e) => {
            panic!("unexpected error: {e}");
        }
    }
}

/// Tests that EOF on empty input returns Eof.
#[tokio::test(flavor = "multi_thread")]
async fn empty_input_eof() {
    let shell_ref = create_test_shell().await;
    let prompt = create_prompt();
    let mut backend = MinimalInputBackend;
    let mut reader = Cursor::new(b"" as &[u8]);

    let result = tokio::task::block_in_place(|| {
        backend.read_line_from(&shell_ref, &prompt, &mut reader, false)
    });

    match result {
        Ok(ReadResult::Eof) => {}
        Ok(_) => {
            panic!("expected Eof, got other ReadResult variant");
        }
        Err(e) => {
            panic!("unexpected error: {e}");
        }
    }
}
