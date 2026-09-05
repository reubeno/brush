//! Interactive behavior tests for the reedline input backend, run over a
//! real pseudo-terminal.
//!
//! `pty_startup_tests.rs` covers *startup* on the default backend and
//! `interactive_tests.rs` covers job control on the `basic` backend; this file
//! is for behavior specific to reedline's line editing (key bindings, its
//! interaction with the terminal).
//!
//! Unlike the basic backend, reedline queries the terminal for the cursor
//! position (DSR, `ESC [ 6 n`) before it paints a prompt. A pty with nothing
//! on the other end never answers, so these tests play the role of the
//! terminal emulator and answer each query themselves (or, for the timeout
//! test, deliberately withhold an answer).

// Only compile this for platforms supported by expectrl's pty backend.
#![cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::time::Duration;

use anyhow::Context;
use expectrl::{
    Expect, Session,
    process::unix::{PtyStream, UnixProcess},
    stream::log::LogStream,
};

const PROMPT: &str = "brush> ";
const DSR_QUERY: &str = "\x1b[6n";
const DSR_REPLY: &str = "\x1b[1;1R";

/// A key bound with `bind -x` must run its command and leave the shell
/// alive. Regression test for reedline >= 0.48 delivering such keys as
/// `Signal::HostCommand`, which previously fell through to a fatal
/// "unexpected error occurred reading input".
#[test]
fn bound_key_runs_command_and_shell_survives() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    // The bound command's output is split so the echoed keystrokes of the
    // `bind` line itself can't satisfy the expectation below.
    session.send_line(r#"bind -x '"\C-t": echo BOUND_""FIRED'"#)?;
    expect_next_prompt(&mut session, 0)?;

    // Ctrl+T.
    session.send("\x14")?;
    session
        .expect("BOUND_FIRED")
        .context("bound command did not run")?;
    expect_next_prompt(&mut session, 0).context("no prompt after bound command")?;

    // The shell must still be interactive afterwards.
    session.send_line("echo STILL_$((40+2))")?;
    session
        .expect("STILL_42")
        .context("shell did not survive the bound command")?;

    Ok(())
}

/// A single unanswered cursor-position query (a transient terminal hiccup,
/// e.g. right after a full-screen program hands the terminal back) must not
/// terminate the shell; the query is retried and the next answer is used.
#[test]
fn transient_cursor_query_timeout_is_retried() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    // Withhold the answer to the query preceding the next prompt; crossterm
    // gives up on it after ~2s and brush must ask again rather than exit.
    session.send_line("echo BEFORE_$((7*6))")?;
    session.expect("BEFORE_42")?;
    expect_next_prompt(&mut session, 1)
        .context("shell did not survive one unanswered cursor query")?;

    session.send_line("echo AFTER_$((6*7))")?;
    session
        .expect("AFTER_42")
        .context("shell not interactive after the retried query")?;

    Ok(())
}

/// The retry is bounded: a terminal that never answers must make the shell
/// give up (three attempts, ~2s each) rather than loop forever.
#[test]
fn unanswered_cursor_queries_eventually_fail_the_read() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    session.send_line("echo BEFORE_$((7*6))")?;
    session.expect("BEFORE_42")?;

    // Never answer. Exactly three queries, then the shell exits on the error.
    for attempt in 1..=3 {
        session
            .expect(DSR_QUERY)
            .with_context(|| format!("no cursor-position query for attempt {attempt}"))?;
    }
    session
        .expect("The cursor position could not be read")
        .context("shell did not report the exhausted query")?;
    session
        .expect(expectrl::Eof)
        .context("shell did not exit after exhausting retries")?;

    Ok(())
}

//
// Helpers
//

type ShellSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stdout>>;

/// Waits for the cursor-position query that precedes a prompt paint, answers
/// it, and then waits for the prompt. The first `withhold` queries are left
/// unanswered instead, so the shell has to re-issue them.
fn expect_next_prompt(session: &mut ShellSession, mut withhold: usize) -> anyhow::Result<()> {
    loop {
        session
            .expect(DSR_QUERY)
            .context("no cursor-position query before prompt")?;
        if withhold > 0 {
            withhold -= 1;
            continue;
        }
        session.send(DSR_REPLY)?;
        session
            .expect(PROMPT)
            .context("no prompt after answered query")?;
        return Ok(());
    }
}

fn start_reedline_session() -> anyhow::Result<ShellSession> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut cmd = std::process::Command::new(shell_path);
    cmd.args([
        "--norc",
        "--noprofile",
        "--no-config",
        "--disable-bracketed-paste",
        "--disable-color",
        "--input-backend=reedline",
    ]);
    cmd.env("PS1", PROMPT);
    cmd.env("TERM", "xterm-256color");

    let session = expectrl::session::Session::spawn(cmd)?;

    // N.B. Replace with `session` directly to disable logging of the session.
    let mut session = expectrl::session::log(session, std::io::stdout())?;

    // The timeout tests deliberately let ~2s crossterm timeouts elapse, up
    // to three in a row (MAX_READ_LINE_ATTEMPTS) for the exhaustion test.
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    Ok(session)
}
