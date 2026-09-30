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

/// OSC 633 markers bracketing a command: emitted before it runs, and after.
const COMMAND_STARTED: &str = "\x1b]633;C";
const COMMAND_FINISHED: &str = "\x1b]633;D";

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

/// Like readline, completion sees the whole line, not just the text before the cursor:
/// e.g. `COMP_WORDS` includes words after it.
#[test]
fn completion_sees_text_after_cursor() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    session.send_line(
        r#"_f() { echo "WORDS""=<${COMP_WORDS[*]}> CWORD=$COMP_CWORD" >&2; }; complete -F _f mycmd"#,
    )?;
    expect_next_prompt(&mut session, 0)?;

    // Type the line, move the cursor back before `b`, and press Tab.
    session.send("mycmd a b")?;
    session.send("\x1b[D\t")?;
    session
        .expect("WORDS=<mycmd a b> CWORD=2")
        .context("completion did not see the whole line")?;

    Ok(())
}

/// Completing a file name in an open quote keeps the quote and, like readline, closes it
/// -- except after a directory, so completion can continue into it.
#[test]
fn completion_closes_open_quote() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("sp ace"), "")?;
    std::fs::write(dir.path().join("q$x"), "")?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("sub").join("x"), "")?;

    let mut session = start_reedline_session_with(|cmd| {
        cmd.current_dir(dir.path());
    })?;
    expect_next_prompt(&mut session, 0)?;

    // Each word is completed, then the line is run; `[%s]` keeps the echoed input from
    // matching.
    for (word, then_typed, expected) in [
        ("'sp", "", "[sp ace]"),
        ("\"q", "", "[q$x]"),
        // The quote stays open after a directory, so finishing the word by hand works.
        ("'su", "x'", "[sub/x]"),
    ] {
        session.send(format!("printf '[%s]\\n' {word}\t"))?;
        answer_cursor_query(&mut session)?;
        session.send_line(then_typed)?;
        session
            .expect(expected)
            .with_context(|| format!("completing {word:?}"))?;
        expect_next_prompt(&mut session, 0)?;
    }

    Ok(())
}

/// Like readline, completing a file name that needs quoting just before a closing quote the
/// user already typed replaces that quote with its own, putting a directory's slash after
/// it. Expected lines were captured from bash 5.3.
#[test]
fn completion_replaces_typed_closing_quote() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("dir with space"))?;
    std::fs::write(dir.path().join("it's"), "")?;
    let mut session = start_reedline_session_with(|cmd| {
        cmd.current_dir(dir.path());
    })?;
    expect_next_prompt(&mut session, 0)?;

    bind_show_line(&mut session)?;

    for (typed, expected) in [
        (r#"echo "di" x"#, r#"LINE=<echo "dir with space"/ x>"#),
        (r#"echo "it" x"#, r#"LINE=<echo "it's" x>"#),
    ] {
        // Type the line, move the cursor back to just before the closing quote, and press
        // Tab.
        expect_completed_line(
            &mut session,
            &format!("{typed}\x1b[D\x1b[D\x1b[D\t"),
            expected,
        )?;
    }

    Ok(())
}

/// Like readline, several candidates are first completed to their longest common prefix.
/// Expected lines were captured from bash 5.3.
#[test]
fn completion_inserts_common_prefix() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("item1"), "")?;
    std::fs::write(dir.path().join("item2"), "")?;
    std::fs::create_dir(dir.path().join("dir a"))?;
    std::fs::create_dir(dir.path().join("dir b"))?;
    let mut session = start_reedline_session_with(|cmd| {
        cmd.current_dir(dir.path());
    })?;
    expect_next_prompt(&mut session, 0)?;

    bind_show_line(&mut session)?;

    for (typed, expected) in [
        ("echo ite", "LINE=<echo item>"),
        (r#"echo "di"#, r#"LINE=<echo "dir >"#),
    ] {
        expect_completed_line(&mut session, &format!("{typed}\t"), expected)?;
    }

    Ok(())
}

/// Like readline, completing a variable whose value is a directory appends a `/`.
#[test]
#[ignore = "TODO(completions): mark variables naming directories with a trailing slash"]
fn completing_directory_variable_appends_slash() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut session = start_reedline_session_with(|cmd| {
        cmd.env("DIRVAR", dir.path());
    })?;
    expect_next_prompt(&mut session, 0)?;

    bind_show_line(&mut session)?;

    // Type a variable reference and press Tab.
    expect_completed_line(&mut session, "echo $DIRV\t", "LINE=<echo $DIRVAR/>")?;

    Ok(())
}

/// Like readline, completing a file name under a `~` or `$VAR` directory keeps the
/// directory as typed, quoting the rest so the directory still expands. Expected lines were
/// captured from bash 5.3.
#[test]
fn completing_under_tilde_or_variable_keeps_directory() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::create_dir(home.path().join("Docs dir"))?;
    let mut session = start_reedline_session_with(|cmd| {
        cmd.env("HOME", home.path());
    })?;
    expect_next_prompt(&mut session, 0)?;

    bind_show_line(&mut session)?;

    for (typed, expected) in [
        ("echo ~/Do", r"LINE=<echo ~/Docs\ dir/>"),
        ("echo $HOME/Do", r#"LINE=<echo "$HOME/Docs dir"/>"#),
    ] {
        expect_completed_line(&mut session, &format!("{typed}\t"), expected)?;
    }

    Ok(())
}

/// `preexec` fires for lines the user typed, not for a command a key binding ran. The
/// hooks exist to observe what the user is about to run; bash-preexec has the same split,
/// because a `bind -x` command runs from readline rather than from the command line.
#[test]
fn bound_key_command_does_not_fire_preexec() -> anyhow::Result<()> {
    let mut session = start_reedline_session_with(|cmd| {
        cmd.arg("--enable-zsh-hooks");
    })?;
    expect_next_prompt(&mut session, 0)?;

    // The hook counts its dispatches as well as reporting its argument, so a stray dispatch
    // for the bound command can't hide between the markers below. Markers are split so the
    // echoed keystrokes of the setup lines can't satisfy the expectations.
    session.send_line(r#"preexec() { n=$((n+1)); echo "PRE_""EXEC[$1]"; }"#)?;
    expect_next_prompt(&mut session, 0)?;

    // Dispatch 1, for this line.
    session.send_line(r#"bind -x '"\C-t": echo BOUND_""FIRED'"#)?;
    expect_next_prompt(&mut session, 0)?;

    // Ctrl+T. The bound command runs, but nothing was typed, so no dispatch.
    session.send("\x14")?;
    session
        .expect("BOUND_FIRED")
        .context("bound command did not run")?;
    expect_next_prompt(&mut session, 0).context("no prompt after bound command")?;

    // Dispatch 2, for this line -- `$n` expands after the hook has already run, so a third
    // dispatch anywhere would show up here.
    session.send_line("echo COUNT_$n")?;
    session
        .expect("PRE_EXEC[echo COUNT_$n]")
        .context("preexec did not fire for a typed line")?;
    session
        .expect("COUNT_2")
        .context("preexec fired for something other than the two typed lines")?;

    Ok(())
}

/// A command a key binding runs is bracketed by the same OSC 633 marker pair as a typed one.
/// Terminals track commands by that pair, so emitting only the closing marker -- as this path
/// used to -- leaves one counting a command that never started.
#[test]
fn osc_command_markers_stay_paired_for_a_bound_command() -> anyhow::Result<()> {
    let mut session = start_reedline_session_with(|cmd| {
        cmd.arg("--enable-terminal-integration");
        // OSC 633 is emitted only for terminals known to understand it.
        cmd.env("TERM_PROGRAM", "vscode");
    })?;
    expect_next_prompt(&mut session, 0)?;

    session.send_line(r#"bind -x '"\C-t": echo BOUND_""FIRED'"#)?;
    expect_next_prompt(&mut session, 0)?;

    // Ctrl+T. The bound command runs without the user typing a line, but it is still a
    // command, so it gets both markers around it.
    session.send("\x14")?;
    session
        .expect(COMMAND_STARTED)
        .context("no command-started marker for the bound command")?;
    session
        .expect("BOUND_FIRED")
        .context("bound command did not run")?;
    session
        .expect(COMMAND_FINISHED)
        .context("no command-finished marker for the bound command")?;

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

/// Binds Ctrl-T to show the line being edited, as `LINE=<...>`, for
/// [`expect_completed_line`].
fn bind_show_line(session: &mut ShellSession) -> anyhow::Result<()> {
    session.send_line(r#"bind -x '"\C-t": echo "LINE""=<$READLINE_LINE>"'"#)?;
    expect_next_prompt(session, 0)
}

/// Types `keys`, which press Tab to complete, then shows the line being edited (see
/// [`bind_show_line`]) and expects it to be `expected`, then clears the line.
fn expect_completed_line(
    session: &mut ShellSession,
    keys: &str,
    expected: &str,
) -> anyhow::Result<()> {
    session.send(keys)?;
    answer_cursor_query(session)?;
    session.send("\x14")?;
    session
        .expect(expected)
        .with_context(|| format!("completing {keys:?}"))?;
    // Clear the whole line for the next case, once reedline has redrawn it.
    answer_cursor_query(session)?;
    session.send("\x05\x15")?;
    Ok(())
}

/// Answers the cursor-position query reedline sends when it completes (e.g. on Tab).
/// Left unanswered, reedline waits ~2s for the answer before it carries on.
fn answer_cursor_query(session: &mut ShellSession) -> anyhow::Result<()> {
    session
        .expect(DSR_QUERY)
        .context("no cursor-position query on completion")?;
    session.send(DSR_REPLY)?;
    Ok(())
}

fn start_reedline_session() -> anyhow::Result<ShellSession> {
    start_reedline_session_with(|_| {})
}

fn start_reedline_session_with(
    configure: impl FnOnce(&mut std::process::Command),
) -> anyhow::Result<ShellSession> {
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
    configure(&mut cmd);

    let session = expectrl::session::Session::spawn(cmd)?;

    // N.B. Replace with `session` directly to disable logging of the session.
    let mut session = expectrl::session::log(session, std::io::stdout())?;

    // The timeout tests deliberately let ~2s crossterm timeouts elapse, up
    // to three in a row (MAX_READ_LINE_ATTEMPTS) for the exhaustion test.
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    Ok(session)
}
