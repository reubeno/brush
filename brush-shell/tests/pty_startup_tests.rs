//! Shell startup tests run over a real pseudo-terminal (pty).
//!
//! These tests verify that the shell becomes responsive — prints a prompt and executes
//! a first command — within a bounded amount of time when attached to a controlling
//! terminal, across the invocation modes real terminal environments use: plain
//! invocation, explicit `-i`, explicit `-l`, presence/absence of rc and profile files,
//! and different process topologies (the shell as the direct pty-attached child vs. a
//! descendant of another pty-attached process).
//!
//! They complement `interactive_tests.rs`, which exercises interactive *behavior* (job
//! control, suspension, pipelines) using the simplified `basic` input backend and
//! argv0-based login detection. This file instead focuses on *startup*, uses the
//! default input backend (the one real terminal sessions get), and exercises the
//! explicit `--login` flag. Startup is where terminal-ownership setup happens
//! (process-group creation, `tcsetpgrp`, `SIGTTIN`/`SIGTTOU` handling), and bugs there
//! tend to appear only with a real controlling terminal — piped stdin bypasses the
//! whole path. (See issue #1318 for an example of such a regression.)

// Only compile this for platforms supported by expectrl's pty backend.
#![cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use expectrl::Expect as _;

mod pty_common;
use pty_common::{
    DEFAULT_PROMPT, PtySession, brush_command, expect_answering_queries, spawn_shell,
};

/// The shell invoked with no flags, as by `exec brush` or a terminal emulator configured
/// with a bare command line; interactivity is inferred from stdin being a terminal.
#[test]
fn default_backend_plain_starts_promptly() -> anyhow::Result<()> {
    let session = spawn_shell(brush_command(&[]))?;
    expect_responsive_shell(session, /* expect_prompt */ true)
}

/// The shell invoked with an explicit `-i`, as scripts and embedders commonly do to force
/// interactive mode.
#[test]
fn default_backend_interactive_flag_starts_promptly() -> anyhow::Result<()> {
    let session = spawn_shell(brush_command(&["-i"]))?;
    expect_responsive_shell(session, /* expect_prompt */ true)
}

/// The shell invoked with an explicit `-l`, as terminal emulators configured to start
/// login shells do. This is a distinct code path from argv0-based login detection, which
/// `login_shell_via_argv0_shows_prompt` in `interactive_tests.rs` covers.
#[test]
fn default_backend_login_flag_starts_promptly() -> anyhow::Result<()> {
    let session = spawn_shell(brush_command(&["-l"]))?;
    expect_responsive_shell(session, /* expect_prompt */ true)
}

/// A login shell for a user with no rc/profile files at all (fresh account, minimal
/// container image): startup must not depend on any dotfile existing. Since system-wide
/// profile files may still run and set their own PS1, this asserts on command output
/// rather than a specific prompt string.
#[test]
fn login_flag_with_empty_home_starts_promptly() -> anyhow::Result<()> {
    let empty_home = tempfile::tempdir()?;

    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let mut cmd = std::process::Command::new(shell_path);
    cmd.args(["-l", "--disable-bracketed-paste", "--disable-color"]);
    cmd.env("HOME", empty_home.path());
    cmd.env("TERM", "linux");

    let session = spawn_shell(cmd)?;
    expect_responsive_shell(session, /* expect_prompt */ false)
}

/// The shell started by a wrapper process rather than directly by whatever owns the pty —
/// as happens under `script(1)`, `su`, terminal multiplexers, and nested-shell setups. The
/// shell is then not the pty's session leader and must take terminal ownership from
/// another process group; spawning it directly (as the other tests here and all of
/// `interactive_tests.rs` do) never exercises that transition, since the direct child *is*
/// the session leader and already owns the terminal.
#[test]
fn login_shell_as_grandchild_starts_promptly() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut cmd = std::process::Command::new("sh");
    // The trailing `exit $?` keeps `sh` from optimizing the command into an exec, so brush
    // really runs as a grandchild of the pty session leader. PS1 is set inline since some
    // `sh` implementations drop it from the environment of non-interactive shells.
    cmd.args([
        "-c",
        "PS1='brush> ' \"$BRUSH_BIN\" -l --norc --noprofile --no-config \
         --disable-bracketed-paste --disable-color; exit $?",
    ]);
    cmd.env("BRUSH_BIN", shell_path);
    cmd.env("TERM", "linux");

    let session = spawn_shell(cmd)?;
    expect_responsive_shell(session, /* expect_prompt */ true)
}

//
// Helpers
//

/// Asserts the shell comes up and responds to input within the expect timeout.
fn expect_responsive_shell(mut session: PtySession, expect_prompt: bool) -> anyhow::Result<()> {
    if expect_prompt {
        expect_answering_queries(&mut session, DEFAULT_PROMPT)
            .context("No prompt appeared within the startup timeout")?;
    }

    session.send_line("echo marker $((6*7))")?;
    expect_answering_queries(&mut session, "marker 42")
        .context("Shell did not respond to input within the startup timeout")?;

    session.send_line("exit")?;
    expect_answering_queries(&mut session, expectrl::Eof).context("Shell did not exit cleanly")?;

    Ok(())
}
