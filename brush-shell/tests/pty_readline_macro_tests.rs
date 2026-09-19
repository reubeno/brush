//! Readline macro tests run over a real pseudo-terminal (pty).
//!
//! These tests intentionally exercise the default reedline/crossterm input path. Macro
//! bindings depend on translating terminal bytes into editor events, so unit tests alone
//! cannot establish that an actual control key invokes the expected macro.

// Only compile this for platforms supported by expectrl's pty backend.
#![cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context as _;
use expectrl::{ControlCode, Expect as _};

mod pty_common;
use pty_common::{
    DEFAULT_PROMPT, PtySession, brush_command, expect_answering_queries, spawn_shell,
};

#[test]
fn readline_macro_inserts_literal_text() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"\C-g": "echo LITERAL-RAN"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    session.send(ControlCode::LineFeed)?;
    expect_answering_queries(&mut session, "LITERAL-RAN\r\n")
        .context("literal macro text was not inserted and executed")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
#[ignore = "reedline dispatches a compound event through its ordinary handler after it enters history search; tracked under #380 pending an upstream reedline change"]
fn readline_macro_enters_history_search_and_accepts_the_match() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line("echo pwd")?;
    expect_answering_queries(&mut session, "\r\npwd\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"\C-g": "\C-r\x70\x77\x64\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "\r\npwd\r\n")
        .context("macro query and accept-line were not dispatched to history search")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line("echo NORMAL-MODE")?;
    expect_answering_queries(&mut session, "\r\nNORMAL-MODE\r\n")
        .context("macro left the following prompt in history-search mode")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session
        .send_line(r#"bind '"\C-g": "\C-r\x70\x77\x64\recho SEARCH-TAIL\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "\r\npwd\r\n")?;
    expect_answering_queries(&mut session, "SEARCH-TAIL\r\n")
        .context("accepting a history match lost the remainder of the macro")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn native_history_search_keeps_its_accept_and_cancel_behavior() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line("echo NATIVE-HISTORY")?;
    expect_answering_queries(&mut session, "\r\nNATIVE-HISTORY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": printf "LINE=[%s]\n" "$READLINE_LINE"; READLINE_LINE='; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // Native Enter selects a match without submitting it.
    session.send("\x12NATIVE-HISTORY\r")?;
    expect_answering_queries(&mut session, "brush> echo NATIVE-HISTORY")?;
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo NATIVE-HISTORY]\r\n")
        .context("native history-search acceptance unexpectedly submitted or lost the match")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("echo ORIGINAL\x12NATIVE-HISTORY")?;
    expect_answering_queries(&mut session, "(rev search: NATIVE-HISTORY)")?;
    session.send("\x1b")?;
    expect_answering_queries(&mut session, "brush> echo ORIGINAL")?;
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo ORIGINAL]\r\n")
        .context("canceling native history search changed the original command")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn bracketed_paste_follows_a_held_macro_prefix() -> anyhow::Result<()> {
    let mut session = spawn_shell(pty_common::brush_command_with_bracketed_paste(&[
        "--input-backend=reedline",
    ]))?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"xy": "MATCH"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("echo x\x1b[200~PASTED\x1b[201~z\r")?;
    expect_answering_queries(&mut session, "xPASTEDz\r\n")
        .context("bracketed paste overtook the held prefix")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send_line(
        r#"bind -x '"\C-t": printf "BEFORE=[%s]\n" "$READLINE_LINE"'; bind '"\C-tx": "LONG"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("echo \x14\x1b[200~PASTED\x1b[201~")?;
    expect_answering_queries(&mut session, "BEFORE=[echo ]\r\n")
        .context("paste was applied before the held host command")?;
    expect_answering_queries(&mut session, "brush> echo PASTED")?;
    session.send("z\r")?;
    expect_answering_queries(&mut session, "PASTEDz\r\n")
        .context("deferred paste was lost or replayed out of order")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn readline_macro_listing_preserves_utf8_when_replayed() -> anyhow::Result<()> {
    let text = "\u{e9}\u{1f600}\u{10d}";
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(format!(
        r#"bind '"\C-g": "echo {text}\r"'; bind "$(bind -s)"; echo BIND-READY"#
    ))?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, format!("\r\n{text}\r\n"))
        .context("macro dump/reload changed UTF-8 text into control or meta keys")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn readline_macro_replays_text_and_editing_commands() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-g": "echo discarded\C-a\C-kecho MIXED-RAN\C-m"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "MIXED-RAN\r\n")
        .context("mixed macro did not replay editing commands and accept the line")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_replays_atuin_chain() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-x\C-_A0\C-g": ""'; bind '"\C-r": "\C-x\C-_A1\C-g\C-x\C-_A0\C-g"'; bind -x '"\C-x\C-_A1\C-g": echo ATUIN-RAN'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("\x12")?;
    expect_answering_queries(&mut session, "ATUIN-RAN\r\n")
        .context("atuin-style macro chain did not invoke its host command")?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_honors_rebinding_made_by_bound_command() -> anyhow::Result<()> {
    // atuin's accept path: the bound command sets the line and rebinds the chain's trailing
    // key to accept-line, which must then run the line.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-x\C-_A0\a": ""'; bind -x '"\C-x\C-_A1\a": READLINE_LINE="echo RAN-VIA-ACCEPT"; READLINE_POINT=${#READLINE_LINE}; bind '"'"'"\C-x\C-_A0\a": accept-line'"'"''; bind '"\C-r": "\C-x\C-_A1\a\C-x\C-_A0\a"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("\x12")?;
    expect_answering_queries(&mut session, "RAN-VIA-ACCEPT\r\n").context(
        "chain key rebound to accept-line during the bound command did not run the line",
    )?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_replays_text_after_bound_command() -> anyhow::Result<()> {
    // Bytes after a bound command are only interpreted once it has run, as in bash.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": echo BOUND-RAN'; bind '"\C-g": "\C-techo AFTER-RAN\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "BOUND-RAN\r\n")
        .context("bound command at the start of the macro did not run")?;
    expect_answering_queries(&mut session, "AFTER-RAN\r\n")
        .context("text after the bound command was not replayed and executed")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_control_question_deletes_backward() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"\C-g": "echo DEL-XX\C-?\C-?\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "DEL-\r\n")
        .context("\\C-? in the macro did not delete the preceding characters")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_chain_survives_repeated_rebinding() -> anyhow::Result<()> {
    // atuin re-runs `bind` on every invocation; the macro must keep resolving the same way.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-x\C-_A0\a": ""'; bind '"\C-r": "\C-x\C-_A1\a\C-x\C-_A0\a"'; bind -x '"\C-x\C-_A1\a": echo ATUIN-RAN; bind '"'"'"\C-x\C-_A0\a": ""'"'"''; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    for _ in 0..3 {
        session.send("\x12")?;
        expect_answering_queries(&mut session, "ATUIN-RAN\r\n")
            .context("atuin-style macro chain stopped invoking its host command")?;
        expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    }

    exit_shell(&mut session)
}

#[test]
fn readline_macro_runs_every_line_it_accepts() -> anyhow::Result<()> {
    // Two accept-lines in one macro body run two commands, as in bash.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"\C-g": "echo FIRST-RAN\recho SECOND-RAN\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "FIRST-RAN\r\n")
        .context("first accepted line did not run")?;
    expect_answering_queries(&mut session, "SECOND-RAN\r\n")
        .context("text after the first accept-line was not replayed and run")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_back_to_back_accept_lines_keep_the_rest() -> anyhow::Result<()> {
    // Each accept-line reached mid-body is carried out by a second, immediately accepting
    // read, with what follows left for the read after. Consecutive accept-lines are the
    // tightest form of that: the replay for one begins with another. Empty lines submit and
    // do nothing, as in bash, and the tail must survive all of them.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session
        .send_line(r#"bind '"\C-g": "echo FIRST-RAN\r\r\r\recho LAST-RAN\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "FIRST-RAN\r\n")
        .context("first accepted line did not run")?;
    expect_answering_queries(&mut session, "LAST-RAN\r\n")
        .context("a run of empty accept-lines swallowed the rest of the macro")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // The shell is still usable afterwards.
    session.send_line("echo STILL-ALIVE")?;
    expect_answering_queries(&mut session, "STILL-ALIVE\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn readline_macro_nested_accept_line_runs_both_lines() -> anyhow::Result<()> {
    // An inner macro that accepts the line, referenced from an outer one with more after
    // it: bash runs both lines.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-t": "echo INNER-RAN\r"'; bind '"\C-g": "\C-techo OUTER-RAN\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "INNER-RAN\r\n")
        .context("inner macro's accepted line did not run")?;
    expect_answering_queries(&mut session, "OUTER-RAN\r\n")
        .context("outer macro's remainder after the nested accept-line did not run")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn shell_expand_line_expands_the_buffer_in_place() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind -x '"\C-t": echo "LINE=[$READLINE_LINE]"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(r#"echo $((6*7)) "$(echo A B)" 'q q'"#)?;
    session.send("\x1b\x05")?; // \M-\C-e: shell-expand-line, bound by default
    expect_answering_queries(&mut session, "brush> echo 42 A B q q")
        .context("shell-expand-line did not repaint the expanded buffer")?;

    // Only once the expansion has been painted: reedline drops keys that arrive in the same
    // batch as a bound command.
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo 42 A B q q]\r\n")
        .context("the expanded buffer was not what the bound command saw")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // The expanded line is still in the buffer, as in bash; clear it before leaving.
    session.send("\x15")?;
    exit_shell(&mut session)
}

#[test]
fn shell_expand_line_inside_a_macro_expands_before_the_rest_replays() -> anyhow::Result<()> {
    // The shape of fzf's and zoxide's widgets: insert a command substitution, expand it in
    // place, then accept the line.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind '"\C-g": "echo `echo WIDGET-RAN`\e\C-e\r"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "WIDGET-RAN\r\n")
        .context("macro with shell-expand-line did not expand and run")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn shell_expand_line_failure_leaves_the_buffer_untouched() -> anyhow::Result<()> {
    // An expansion that fails reports the error and leaves the line as it was, as in bash.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind -x '"\C-t": echo "LINE=[$READLINE_LINE]"'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("echo ${nope?boom} KEPT")?;
    session.send("\x1b\x05")?; // \M-\C-e: shell-expand-line
    expect_answering_queries(&mut session, "boom")
        .context("the failed expansion was not reported")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo ${nope?boom} KEPT]\r\n")
        .context("the buffer was changed by a failed expansion")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("\x15")?;
    exit_shell(&mut session)
}

#[test]
fn event_needing_the_editor_is_dropped_after_a_bound_command() -> anyhow::Result<()> {
    // Pins a documented divergence (docs/reference/key-bindings.md): after a bound command,
    // macro bytes that need reedline's read loop (here \C-r, history search) are dropped,
    // and the bytes after them still take effect. bash would enter history search instead.
    // `history_search_after_a_bound_command_matches_bash` below is the test for the fix.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": echo BOUND-RAN'; bind '"\C-g": "\C-t\C-recho TAIL-RAN\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "BOUND-RAN\r\n")
        .context("bound command at the start of the macro did not run")?;
    expect_answering_queries(&mut session, "TAIL-RAN\r\n")
        .context("text after the dropped history-search key was not replayed and run")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
#[ignore = "reedline cannot replay events that need its read loop after a bound command; tracked under #380, see docs/reference/key-bindings.md"]
fn history_search_after_a_bound_command_matches_bash() -> anyhow::Result<()> {
    // Checked against bash 5.3: \C-r after the bound command starts a reverse search, the
    // text that follows is the search string, and accept-line runs the line found. The
    // search string is spelled with a hex escape so the bind line itself doesn't match it.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line("echo FOUND-LINE")?;
    expect_answering_queries(&mut session, "FOUND-LINE\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": echo BOUND-RAN'; bind '"\C-g": "\C-t\C-r\x46OUND\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "BOUND-RAN\r\n")
        .context("bound command at the start of the macro did not run")?;
    expect_answering_queries(&mut session, "FOUND-LINE\r\n")
        .context("history search after the bound command did not recall and run the line")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn escape_dismisses_the_completion_menu_while_a_meta_macro_key_is_bound() -> anyhow::Result<()> {
    // fzf binds \ec, which makes a lone Esc the start of a key sequence. There is no
    // key-sequence timeout, so a held Esc would leave the completion menu open until the
    // next key; instead its default meaning fires at once and the line is repainted without
    // the menu. Held, Esc produces no output at all and the repaint never comes.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\ec": "echo FZF-RAN\r"'; cd "$(mktemp -d)" && mkdir aa ab; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("cd a")?;
    session.send("\t")?;
    expect_answering_queries(&mut session, "ab").context("completion menu did not open")?;

    session.send("\x1b")?;
    expect_answering_queries(&mut session, "brush> cd a")
        .context("Esc was held for the meta key instead of dismissing the menu")?;

    // The meta key still completes after a lone Esc.
    session.send("\x15")?;
    session.send("\x1b")?;
    session.send("c")?;
    expect_answering_queries(&mut session, "FZF-RAN\r\n")
        .context("Esc followed by c did not fire the \\ec macro")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn a_lone_escape_prefixes_the_next_key_into_a_meta_key() -> anyhow::Result<()> {
    // readline's meta prefix: the terminal reports Esc and the key after it as two events,
    // and the pair is one meta key. Every expectation here was checked against bash.
    //
    // Every key here is followed by a wait for the repaint it triggers, one wait per paint.
    // That is what a human's pause between two keystrokes does, and this has to reproduce
    // it twice over. Bytes written together reach crossterm in one read and come back
    // already paired, which would test its batching rather than anything here. And a wait
    // that matches the paint of an *earlier* key leaves that key's cursor-position query
    // unanswered, so the reply goes out later -- into the same byte stream this test types
    // on, beginning with an escape, landing in the middle of the escape below.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session
        .send_line(r#"bind -x '"\C-t": printf "LINE=[%s]\n" "$READLINE_LINE"; READLINE_LINE='"#)?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // Esc then b is backward-word, so the X lands before `bb` rather than being typed
    // after an escape that did nothing.
    session.send("echo aa bb")?;
    expect_answering_queries(&mut session, "brush> echo aa bb")?;
    session.send("\x1b")?;
    expect_answering_queries(&mut session, "brush> echo aa bb")?;
    session.send("b")?;
    session.send("X")?;
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo aa Xbb]\r\n")
        .context("Esc followed by b was not backward-word")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // Esc is also what dismisses a completion menu and cancels a history search, so a key
    // the pair is not bound for must still reach the editor after one: `\e\C-t` is bound
    // to nothing, and the bound command on `\C-t` runs rather than being swallowed.
    session.send("echo KEEPME")?;
    expect_answering_queries(&mut session, "brush> echo KEEPME")?;
    session.send("\x1b")?;
    expect_answering_queries(&mut session, "brush> echo KEEPME")?;
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo KEEPME]\r\n")
        .context("an unbound meta pair swallowed the key after the escape")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    // Two more properties are covered at the event level instead, where the delivery of
    // each key is explicit: a run of escapes pairing off rather than each one prefixing
    // the next key (`escapes_pair_off_before_prefixing_the_next_key`), and a held escape
    // being dropped at the read boundary (`a_held_escape_is_dropped_when_the_read_ends`)
    // -- crossing one means submitting a line, and Esc plus the Enter that would do it is
    // itself a key.

    exit_shell(&mut session)
}

#[test]
fn multi_key_bound_command_runs_from_the_keyboard() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(r#"bind -x '"\C-x\C-r": echo MULTI-KEY-RAN'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("\x18")?;
    session.send("\x12")?;
    expect_answering_queries(&mut session, "MULTI-KEY-RAN\r\n")
        .context("two-key bound command did not run when typed")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

#[test]
fn shift_enter_keeps_the_line_open_even_when_enter_has_a_macro() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"\C-m": "echo WRONG-ENTER\r"'; bind -x '"\C-t": printf "LINE=[%s]\n" "$READLINE_LINE"; READLINE_LINE='; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("echo FIRST")?;
    session.send("\x1b[13;2u")?;
    session.send("echo SECOND")?;
    session.send("\x14")?;
    expect_answering_queries(&mut session, "LINE=[echo FIRST\r\necho SECOND]\r\n")
        .context("Shift+Enter submitted the line or invoked the Enter macro")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("exit\n")?;
    expect_answering_queries(&mut session, expectrl::Eof)
}

#[test]
fn empty_macro_and_action_triggers_do_not_stall_replay() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"": ""'; bind '"\C-t": ""'; bind '"\C-g": "\C-techo EMPTY-MACRO-RAN\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("\x07")?;
    expect_answering_queries(&mut session, "EMPTY-MACRO-RAN\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send_line(
        r#"bind '"": accept-line'; bind '"\C-g": "\C-techo EMPTY-ACTION-RAN\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("\x07")?;
    expect_answering_queries(&mut session, "EMPTY-ACTION-RAN\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn macro_trigger_spans_nested_and_keyboard_input() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind '"xy": "OK"'; bind '"\C-t": "x"'; bind '"\C-g": "echo \C-ty\r"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("\x07")?;
    expect_answering_queries(&mut session, "OK\r\n")
        .context("nested macro boundary split a multi-key trigger")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("echo ")?;
    session.send("\x14")?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    session.send("y\n")?;
    expect_answering_queries(&mut session, "OK\r\n")
        .context("keyboard input did not complete the macro's pending trigger")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn prefix_fallback_retains_input_across_host_and_accept_line() -> anyhow::Result<()> {
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": echo SHORT'; bind -x '"\C-t\C-r": echo LONG'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("echo ")?;
    session.send("\x14")?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    session.send("z")?;
    expect_answering_queries(&mut session, "SHORT\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("\n")?;
    expect_answering_queries(&mut session, "z\r\n")
        .context("fallback host command lost the key following its trigger")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send_line(r#"bind '"\C-t": accept-line'; echo BIND-READY"#)?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("echo FIRST")?;
    session.send("\x14")?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    session.send("e")?;
    expect_answering_queries(&mut session, "FIRST\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send("cho SECOND-$((6*7))\n")?;
    expect_answering_queries(&mut session, "SECOND-42\r\n")
        .context("fallback accept-line lost the next line's first key")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    exit_shell(&mut session)
}

#[test]
fn bind_x_sees_readline_line_even_when_empty() -> anyhow::Result<()> {
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-g": echo "LINE=[${READLINE_LINE-unset}] POINT=[${READLINE_POINT-unset}]"; READLINE_LINE='; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "LINE=[] POINT=[0]\r\n")
        .context("READLINE_LINE was not set for an empty edit buffer")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send("ab")?;
    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "LINE=[ab] POINT=[2]\r\n")
        .context("READLINE_LINE did not reflect the edit buffer")?;

    exit_shell(&mut session)
}

#[test]
fn edits_before_a_nested_bound_command_survive_the_return_to_the_host() -> anyhow::Result<()> {
    // A macro whose body edits and then triggers a bound command resolves to a single
    // `Multiple([Edit(..), ExecuteHostCommand(..)])`. Everything deferred rests on reedline
    // applying the edit, returning to us on the command nested after it, and still holding
    // that edit when the read resumes -- none of which upstream promises.
    let mut session = start_shell()?;

    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(
        r#"bind -x '"\C-t": echo BOUND-RAN'; bind '"\C-g": "echo KEPT-$((6*7))\C-t"'; echo BIND-READY"#,
    )?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "BOUND-RAN\r\n")
        .context("the command nested after an edit did not return to the host")?;

    // The buffer the edit left behind is still there for the read that resumes.
    session.send("\n")?;
    expect_answering_queries(&mut session, "KEPT-42\r\n")
        .context("the edit before the nested bound command was lost")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

/// Bytes whose key crossterm decides, one per arm and edge of the single-byte table in
/// `keys::lift_key`: as `bind` spells them, as the terminal sends them, and the character
/// each one's macro inserts.
const CONTROL_BYTES: &[(&str, &str, char)] = &[
    (r"\000", "\u{0}", 'a'),  // Ctrl+Space
    (r"\001", "\u{1}", 'b'),  // Ctrl+A, bottom of the letter range
    (r"\032", "\u{1a}", 'c'), // Ctrl+Z, top of it
    (r"\034", "\u{1c}", 'd'), // Ctrl+4, bottom of the punctuation range
    (r"\037", "\u{1f}", 'e'), // Ctrl+7, top of it
    (r"\177", "\u{7f}", 'f'), // Backspace
];

#[test]
fn control_bytes_reach_the_key_crossterm_reports_them_as() -> anyhow::Result<()> {
    // The single-byte half of `keys::lift_key` mirrors crossterm's own parse table, which
    // crossterm does not expose for us to call. Typing each byte at a real terminal is what
    // ties the two together: the byte is parsed by crossterm and spelled back by our table
    // before the trie is consulted, so a byte crossterm came to report differently would
    // no longer find its macro.
    let mut session = start_shell()?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    let binds: Vec<String> = CONTROL_BYTES
        .iter()
        .map(|(spelling, _, inserted)| std::format!(r#"bind '"{spelling}": "{inserted}"'"#))
        .collect();
    session.send_line(std::format!("{}; echo BIND-READY", binds.join("; ")))?;
    expect_answering_queries(&mut session, "BIND-READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    let keystrokes: String = CONTROL_BYTES.iter().map(|(_, sent, _)| *sent).collect();
    let expected: String = CONTROL_BYTES
        .iter()
        .map(|(_, _, inserted)| inserted)
        .collect();
    session.send("echo ")?;
    session.send(keystrokes.as_str())?;
    session.send("\n")?;
    expect_answering_queries(&mut session, std::format!("{expected}\r\n"))
        .context("a control byte did not reach the key its macro is bound to")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;

    exit_shell(&mut session)
}

fn start_shell() -> anyhow::Result<PtySession> {
    spawn_shell(brush_command(&["--input-backend=reedline"]))
}

fn exit_shell(session: &mut PtySession) -> anyhow::Result<()> {
    session.send_line("exit")?;
    expect_answering_queries(session, expectrl::Eof).context("shell did not exit cleanly")
}
