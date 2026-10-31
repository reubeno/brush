//! Help printed to a pseudo-terminal (pty) is wrapped to the terminal's width.

// Only compile this for platforms supported by expectrl's pty backend.
#![cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::time::Duration;

use expectrl::{Expect, Session};

/// Runs `script` with `brush -c` in a terminal `cols` wide and returns its
/// output.
fn run_in_terminal(cols: u16, script: &str) -> anyhow::Result<String> {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("brush"));
    cmd.args(["--norc", "--noprofile", "--no-config", "-c", script]);
    // No COLUMNS, NO_COLOR or COLORTERM from the caller: only the terminal decides.
    cmd.env_clear().env("TERM", "xterm");

    let mut session = Session::spawn(cmd)?;
    session.set_expect_timeout(Some(Duration::from_secs(8)));
    // The script reads a line first, so it runs only once the size is set.
    session.get_process_mut().set_window_size(cols, 24)?;
    session.send_line("")?;
    let output = session.expect(expectrl::Eof)?;
    Ok(String::from_utf8_lossy(output.as_bytes()).into_owned())
}

/// The width of the widest line, without escape sequences.
fn widest_line(output: &str) -> usize {
    strip_ansi_escapes::strip_str(output)
        .lines()
        .map(|line| line.trim_end().chars().count())
        .max()
        .unwrap_or(0)
}

#[test]
fn builtin_help_wraps_to_the_terminal() -> anyhow::Result<()> {
    let output = run_in_terminal(40, "read; brushctl --help")?;
    assert!(
        strip_ansi_escapes::strip_str(&output).contains("Usage: brushctl"),
        "{output}"
    );
    assert!(widest_line(&output) <= 40, "{output}");
    Ok(())
}
