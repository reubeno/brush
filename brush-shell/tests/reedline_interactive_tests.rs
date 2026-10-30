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

#[test]
fn no_editing_reads_complete_programs() -> anyhow::Result<()> {
    use std::{io::Write as _, process::Stdio};

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("brush"))
        .args([
            "--no-config",
            "--norc",
            "--noprofile",
            "--input-backend=reedline",
            "--noediting",
            "-i",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(
        b"if false\nthen\necho WRONG_BRANCH\nfi\ncat <<EOF\nhere-doc\nEOF\nprintf '%s\\n' con\\\ntinued\nexit\n",
    )?;
    let output = child.wait_with_output()?;
    assert_eq!(String::from_utf8(output.stdout)?, "here-doc\ncontinued\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn editor_family_survives_bound_commands() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    session.send_line(r#"bind -m emacs-standard '"\C-t":vi-editing-mode'; bind -m vi-insert -x '"\C-g":echo FAMILY_""KEPT'"#)?;
    expect_next_prompt(&mut session, 0)?;
    session.send("\x14")?;
    // Reedline parses a terminal burst before applying SwitchMode. Wait until
    // the switch has painted before sending a key for the newly active family.
    session.expect(PROMPT)?;
    for _ in 0..2 {
        session.send("\x07")?;
        session.expect("FAMILY_KEPT")?;
        expect_next_prompt(&mut session, 0)?;
    }
    Ok(())
}

#[test]
fn vi_operators_edit_the_buffer() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    session.send_line(
        r#"set -o vi; bind -m vi-command -x '"\C-g":printf "BUFFER=<%s>\n" "$READLINE_LINE"'"#,
    )?;
    expect_next_prompt(&mut session, 0)?;
    session.send("abc def")?;
    session.expect("brush> abc def")?;
    session.send("\x1b")?;
    session.expect(PROMPT)?;
    session.send("0dw")?;
    session.send("\x07")?;
    session.expect("BUFFER=<def>")?;
    Ok(())
}

#[test]
fn explicit_same_option_selection_overrides_a_live_family_switch() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    session.send_line(r#"set -o emacs; bind -m emacs-standard '"\C-t":vi-editing-mode'; bind -m emacs-standard -x '"\C-g":echo EMACS_""SELECTED'; bind -m vi-insert -x '"\C-g":set -o emacs'"#)?;
    expect_next_prompt(&mut session, 0)?;
    session.send("\x14")?;
    session.expect(PROMPT)?;
    session.send("\x07")?;
    expect_next_prompt(&mut session, 0)?;
    session.send("\x07")?;
    session.expect("EMACS_SELECTED")?;
    Ok(())
}

#[test]
fn vi_visual_selection_deletes_the_selected_text() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    session.send_line(
        r#"set -o vi; bind -m vi-command -x '"\C-g":printf "BUFFER=<%s>\n" "$READLINE_LINE"'"#,
    )?;
    expect_next_prompt(&mut session, 0)?;
    session.send("abc def")?;
    session.expect("brush> abc def")?;
    session.send("\x1b")?;
    session.expect(PROMPT)?;
    for key in ["0", "v", "l", "d"] {
        session.send(key)?;
        session.expect(PROMPT)?;
    }
    session.send("\x07")?;
    session.expect("BUFFER=<c def>")?;
    Ok(())
}

#[test]
fn literal_macro_inserts_into_the_live_buffer() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    session.send_line(r#"bind -m emacs-standard '"\C-t":"echo MACRO_OK"'"#)?;
    expect_next_prompt(&mut session, 0)?;
    session.send("\x14")?;
    session.expect("brush> echo MACRO_OK")?;
    session.send_line("")?;
    session.expect("\r\nMACRO_OK\r\n")?;
    Ok(())
}

#[test]
fn external_editor_uses_current_shell_state_and_cleans_up() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let script = directory.path().join("editor script");
    let record = directory.path().join("record");
    std::fs::write(
        &script,
        r#"printf '%s\n' "$1" "$PWD" "$BRUSH_EDITOR_MARKER" "$2" > "$BRUSH_EDITOR_RECORD"
printf 'echo EDITOR_""EXECUTED\n' > "$2"
"#,
    )?;
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;
    // VISUAL is deliberately selected after startup and contains both a quoted
    // script path and a quoted argument. The command must see shell-local cwd/env.
    session.send_line(format!(r#"set -o vi; VISUAL="/bin/sh '{}' 'two words'"; export BRUSH_EDITOR_MARKER=from-shell BRUSH_EDITOR_RECORD='{}'; cd '{}'; bind -m vi-command '"\C-t":vi-edit-and-execute-command'"#, script.display(), record.display(), directory.path().display()))?;
    expect_next_prompt(&mut session, 0)?;
    session.send("discard me")?;
    session.expect("brush> discard me")?;
    session.send("\x1b")?;
    session.expect(PROMPT)?;
    session.send("\x14")?;
    session.expect(DSR_QUERY)?;
    session.send(DSR_REPLY)?;
    session.expect("\r\nEDITOR_EXECUTED\r\n")?;
    expect_next_prompt(&mut session, 0)?;
    let recorded = std::fs::read_to_string(record)?;
    let lines: Vec<_> = recorded.lines().collect();
    assert_eq!(
        &lines[..3],
        [
            "two words",
            directory.path().to_str().unwrap(),
            "from-shell"
        ]
    );
    let buffer = std::path::Path::new(lines[3]);
    assert!(buffer.is_file());
    session.send("\x04")?;
    session.expect(expectrl::Eof)?;
    assert!(
        !buffer.exists(),
        "editor buffer leaked after shell exit: {}",
        buffer.display()
    );
    assert!(
        !buffer.parent().unwrap().exists(),
        "private editor directory leaked"
    );
    Ok(())
}

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
/// terminate the shell; reedline asks again and uses the next answer.
#[test]
fn transient_cursor_query_timeout_is_tolerated() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    // Withhold the answer to the query preceding the next prompt; crossterm
    // gives up on it after ~2s and the shell must carry on rather than exit.
    session.send_line("echo BEFORE_$((7*6))")?;
    session.expect("BEFORE_42")?;
    expect_next_prompt(&mut session, 1)
        .context("shell did not survive one unanswered cursor query")?;

    session.send_line("echo AFTER_$((6*7))")?;
    session
        .expect("AFTER_42")
        .context("shell not interactive after the unanswered query")?;

    Ok(())
}

/// A terminal that never answers cursor-position queries still gets a
/// prompt and a working shell; reedline (>= 0.52) falls back to anchoring
/// the prompt at the bottom of the screen instead of failing the read.
#[test]
fn unanswered_cursor_queries_still_yield_a_prompt() -> anyhow::Result<()> {
    let mut session = start_reedline_session()?;
    expect_next_prompt(&mut session, 0)?;

    session.send_line("echo BEFORE_$((7*6))")?;
    session.expect("BEFORE_42")?;

    // Never answer from here on.
    session
        .expect(PROMPT)
        .context("no prompt without cursor-query answers")?;
    session.send_line("echo AFTER_$((6*7))")?;
    session
        .expect("AFTER_42")
        .context("shell not interactive without cursor-query answers")?;

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

    // The timeout tests deliberately let ~2s crossterm timeouts elapse,
    // several in a row when the terminal never answers.
    session.set_expect_timeout(Some(Duration::from_secs(15)));

    Ok(session)
}
