//! Whole-buffer expansion and replacement regressions over a real pseudo-terminal.

#![cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd"
))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use expectrl::{ControlCode, Expect as _};

mod pty_common;
use pty_common::{
    DEFAULT_PROMPT, PtySession, brush_command, expect_answering_queries, spawn_shell,
};

/// Oldest bash whose `shell-expand-line` the expectations here were taken from.
const MIN_ORACLE_VERSION: (u32, u32) = (5, 2);

fn start_shell(setup: &str) -> anyhow::Result<PtySession> {
    start_shell_with_command(brush_command(&["--input-backend", "reedline"]), setup)
}

fn start_shell_with_command(
    mut command: std::process::Command,
    setup: &str,
) -> anyhow::Result<PtySession> {
    command.env("HISTFILE", "/dev/null");
    let mut session = spawn_shell(command)?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    session.send_line(format!(
        r#"{setup}; bind -x '"\C-t": printf "LINE=<%s> POINT=<%s>\n" "$READLINE_LINE" "$READLINE_POINT"; READLINE_LINE=; READLINE_POINT=0'; echo READY"#
    ))?;
    expect_answering_queries(&mut session, "READY\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    Ok(session)
}

/// Every case the unit tests pin for [`brush_core::expansion::shell_expand_line`]'s field
/// handling, run through a real editor so the same table can be checked against bash.
const FIELD_BOUNDARY_CASES: &[(&str, &str, &str)] = &[
    // Boundaries between the fields an expansion already had.
    ("IFS=; A=(aa '' bb)", "echo ${A[@]}", "echo aa bb"),
    ("IFS=; A=(aa '' bb)", "echo \"${A[@]}\"", "echo aa  bb"),
    ("IFS=; A=(aa '' bb)", "echo \"${A[*]}\"", "echo aabb"),
    ("IFS=:; A=(aa '' bb)", "echo ${A[@]}", "echo aa  bb"),
    ("IFS=:; A=(aa '' bb)", "echo ${A[*]}", "echo aa  bb"),
    ("IFS=:; set -- aa '' bb", "echo $@", "echo aa  bb"),
    ("IFS=:; set -- aa '' bb", "echo $*", "echo aa  bb"),
    (
        "IFS=:; A=(aa '' bb); set -- aa '' bb",
        "echo ${A[@]} ${A[*]} $@ $*",
        "echo aa  bb aa  bb aa  bb aa  bb",
    ),
    ("IFS=:; A=(aa '' '')", "echo ${A[@]}", "echo aa "),
    ("IFS=:; A=(aa ':bb')", "echo ${A[@]}", "echo aa  bb"),
    ("IFS=': '; A=(aa '' bb)", "echo ${A[@]}", "echo aa  bb"),
    ("IFS=' :'; A=('aa:' ':bb')", "echo ${A[@]}", "echo aa  bb"),
    // Splitting within one scalar expansion: IFS whitespace collapses and is dropped at
    // either end; a non-whitespace IFS character delimits on its own, keeping the empty
    // fields between.
    ("IFS=$' \\t\\n'; V='  x  y  '", "$V", "x y"),
    ("IFS=$' \\t\\n'; V='  x  y  '", "echo $V", "echo  x y"),
    (
        "IFS=$' \\t\\n'; V='  x  y  '",
        "echo $V end",
        "echo  x y  end",
    ),
    ("IFS=$' \\t\\n'; V='  x  y  '", "a${V}b", "a x y b"),
    ("IFS=:; V=:", "echo $V", "echo "),
    ("IFS=:; V=':x::y:'", "$V", " x  y"),
    ("IFS=:; V=':x::y:'", "echo $V", "echo  x  y"),
    ("IFS=:; V=':x::y:'", "a${V}b", "a x  y b"),
    ("IFS=:; V=':x::y:'", "$V$V", " x  y  x  y"),
    ("IFS=:; V=':x::y:'; E=", "${V}$E", " x  y"),
    ("IFS=:; V=':x::y:'", "${V}\"\"", " x  y "),
    ("IFS=$' :\\t\\n'; V=' : x :: y : '", "a${V}b", "a x  y b"),
    ("IFS=; V=' x:y '", "echo $V", "echo  x:y "),
];

/// Cases `shell-expand-line` does not yet match bash on. bash treats an unquoted array or
/// positional expansion as its elements joined on the first IFS character and then
/// field-split, so a boundary next to an empty element or a delimiter behaves like that
/// character: under `IFS=' :'` empty elements vanish and a `:` beside a boundary folds into
/// it, and under `IFS=:` a `:` ending an element leaves an empty field. The shared execution
/// splitter instead keeps every empty element and splits each element on its own. The
/// known-failure cases in `compat/ifs.yaml` pin the same gaps for command execution.
///
/// Columns: setup, line, what bash produces, what brush produces today. The bash column is
/// checked against real bash by [`bash_shell_expand_line_field_boundaries`]; the brush
/// column is asserted by [`shell_expand_line_ifs_gaps_are_known`], so a fix makes that test
/// fail and get updated. Once a case matches, move it into `FIELD_BOUNDARY_CASES` with its
/// bash column.
const IFS_GAP_CASES: &[(&str, &str, &str, &str)] = &[
    (
        "IFS=' :'; A=(aa '' bb)",
        "echo ${A[@]}",
        "echo aa bb",
        "echo aa  bb",
    ),
    (
        "IFS=' :'; set -- aa '' bb",
        "echo $@",
        "echo aa bb",
        "echo aa  bb",
    ),
    (
        "IFS=' :'; A=(aa ':bb')",
        "echo ${A[@]}",
        "echo aa bb",
        "echo aa  bb",
    ),
    (
        "IFS=' :'; A=(aa ':')",
        "echo ${A[@]}",
        "echo aa",
        "echo aa ",
    ),
    (
        "IFS=' ,:'; A=(aa ' :bb')",
        "echo ${A[@]}",
        "echo aa bb",
        "echo aa  bb",
    ),
    (
        "IFS=:; A=('aa:' bb)",
        "echo ${A[@]}",
        "echo aa  bb",
        "echo aa bb",
    ),
];

/// A tilde prefix expands only at the very start of the line, because bash expands the
/// whole line as one word; checked against bash like the cases above.
const TILDE_CASES: &[(&str, &str, &str)] = &[
    ("HOME=/hh; V=val", "~/x $V", "/hh/x val"),
    ("HOME=/hh; V=val", "~ $V", "~ val"),
    ("HOME=/hh; V=val", "echo ~ $V", "echo ~ val"),
    ("HOME=/hh; V=val", "a=~/x $V", "a=~/x val"),
];

fn field_boundaries(
    mut session: PtySession,
    wait_for_redraw: bool,
    cases: &[(&str, &str, &str)],
) -> anyhow::Result<()> {
    for &(setup, line, expected) in cases {
        session.send_line(format!("{setup}; echo CASE-READY"))?;
        expect_answering_queries(&mut session, "CASE-READY\r\n")?;
        expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
        session.send(line)?;
        session.send("\x1b\x05")?;
        if wait_for_redraw {
            expect_answering_queries(&mut session, format!("{DEFAULT_PROMPT}{expected}"))?;
        }
        session.send("\x14")?;
        expect_answering_queries(
            &mut session,
            format!("LINE=<{expected}> POINT=<{}>\r\n", expected.len()),
        )?;
        if wait_for_redraw {
            expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
        }
    }
    session.send_line("exit")?;
    session.expect(expectrl::Eof)?;
    Ok(())
}

#[test]
fn shell_expand_line_field_boundaries_match_bash() -> anyhow::Result<()> {
    field_boundaries(start_shell(":")?, true, FIELD_BOUNDARY_CASES)
}

#[test]
fn shell_expand_line_tilde_prefix_matches_bash() -> anyhow::Result<()> {
    field_boundaries(start_shell(":")?, true, TILDE_CASES)
}

#[test]
fn shell_expand_line_ifs_gaps_are_known() -> anyhow::Result<()> {
    // Asserts brush's current output; see `IFS_GAP_CASES` for what has to change.
    let today: Vec<_> = IFS_GAP_CASES
        .iter()
        .map(|&(setup, line, bash, today)| {
            assert_ne!(bash, today, "no longer a gap: {setup} / {line}");
            (setup, line, today)
        })
        .collect();
    field_boundaries(start_shell(":")?, true, &today)
}

#[test]
fn bash_shell_expand_line_field_boundaries() -> anyhow::Result<()> {
    // The expectations above are bash's; this is what makes that claim checkable. Skipped
    // where there is no bash new enough to compare against, rather than failed.
    let Some(command) = pty_common::bash_oracle_command(MIN_ORACLE_VERSION)? else {
        eprintln!("skipping: no bash oracle available");
        return Ok(());
    };

    let cases: Vec<_> = FIELD_BOUNDARY_CASES
        .iter()
        .chain(TILDE_CASES)
        .copied()
        .chain(
            IFS_GAP_CASES
                .iter()
                .map(|&(setup, line, bash, _)| (setup, line, bash)),
        )
        .collect();
    field_boundaries(start_shell_with_command(command, ":")?, false, &cases)
}

fn inspect_and_exit(session: &mut PtySession, expected: &'static str) -> anyhow::Result<()> {
    session.send("\x14")?;
    expect_answering_queries(session, expected)?;
    expect_answering_queries(session, DEFAULT_PROMPT)?;
    session.send_line("exit")?;
    session.expect(expectrl::Eof)?;
    Ok(())
}

#[test]
fn shell_expand_line_keeps_literal_ifs_delimiters() -> anyhow::Result<()> {
    let mut session = start_shell("IFS=:; V=x:y")?;
    session.send("echo a:b $V")?;
    session.send("\x1b\x05")?;
    expect_answering_queries(&mut session, "brush> echo a:b x y")?;
    inspect_and_exit(&mut session, "LINE=<echo a:b x y> POINT=<12>\r\n")
}

#[test]
fn shell_expand_line_does_not_rerun_prompt_hooks() -> anyhow::Result<()> {
    let mut session = start_shell_with_command(
        brush_command(&["--input-backend", "reedline", "--enable-zsh-hooks"]),
        "N=0; PRE=0; PROMPT_COMMAND='N=$((N+1))'; pre_prompt() { PRE=$((PRE+1)); }; precmd_functions=(pre_prompt)",
    )?;
    for _ in 0..3 {
        session.send("\x15echo $N $PRE")?;
        session.send("\x1b\x05")?;
        expect_answering_queries(&mut session, "brush> echo 1 1")?;
    }
    inspect_and_exit(&mut session, "LINE=<echo 1 1> POINT=<8>\r\n")
}

#[test]
fn shell_expand_line_error_does_not_restart_the_prompt() -> anyhow::Result<()> {
    let mut session = start_shell("N=0; PROMPT_COMMAND='N=$((N+1))'; unset MISSING")?;
    session.send("echo ${MISSING:?EXPANSION-FAILED}")?;
    session.send("\x1b\x05")?;
    expect_answering_queries(&mut session, "error: expansion error: EXPANSION-FAILED\r\n")?;
    expect_answering_queries(&mut session, "brush> echo ${MISSING:?EXPANSION-FAILED}")?;
    session.send("\x15echo $N")?;
    session.send("\x1b\x05")?;
    expect_answering_queries(&mut session, "brush> echo 1")?;
    inspect_and_exit(&mut session, "LINE=<echo 1> POINT=<6>\r\n")
}

#[test]
fn shell_expand_line_replaces_a_multiline_buffer() -> anyhow::Result<()> {
    let mut session = start_shell("V=ok")?;
    session.send("echo \"$V")?;
    session.send(ControlCode::LineFeed)?;
    expect_answering_queries(&mut session, "echo \"$V")?;
    session.send("bar\"")?;
    // A typed newline inside an open quote is part of this same edit buffer.
    expect_answering_queries(&mut session, "bar\"")?;
    session.send("\x1b\x05")?;
    expect_answering_queries(&mut session, "brush> echo ok")?;
    inspect_and_exit(&mut session, "LINE=<echo ok\r\nbar> POINT=<11>\r\n")
}

#[test]
fn shell_expand_line_preserves_command_boundaries() -> anyhow::Result<()> {
    let mut session = start_shell(
        r#"V=ok; bind -x '"\C-g": READLINE_LINE=$'"'"'echo $V\necho bar'"'"'; READLINE_POINT=999'"#,
    )?;
    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "brush> echo $V")?;
    session.send("\x1b\x05")?;
    expect_answering_queries(&mut session, "brush> echo ok")?;
    inspect_and_exit(&mut session, "LINE=<echo ok\r\necho bar> POINT=<16>\r\n")
}

#[test]
fn readline_line_replaces_all_lines_and_clamps_point() -> anyhow::Result<()> {
    let mut session = start_shell(
        r#"bind -x '"\C-g": READLINE_LINE=$'"'"'a\u00e9\n\u754cz'"'"'; READLINE_POINT=999'"#,
    )?;
    session.send("echo \"old")?;
    session.send(ControlCode::LineFeed)?;
    expect_answering_queries(&mut session, "echo \"old")?;
    session.send("tail\"")?;
    expect_answering_queries(&mut session, "tail\"")?;
    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "brush> a\u{e9}")?;
    inspect_and_exit(&mut session, "LINE=<a\u{e9}\r\n\u{754c}z> POINT=<8>\r\n")
}

#[test]
fn readline_line_can_clear_a_multiline_buffer() -> anyhow::Result<()> {
    let mut session =
        start_shell(r#"bind -x '"\C-g": READLINE_LINE=; READLINE_POINT=999; echo CLEARED'"#)?;
    session.send("echo \"old")?;
    session.send(ControlCode::LineFeed)?;
    expect_answering_queries(&mut session, "echo \"old")?;
    session.send("tail\"")?;
    expect_answering_queries(&mut session, "tail\"")?;
    session.send(ControlCode::Bell)?;
    expect_answering_queries(&mut session, "CLEARED\r\n")?;
    expect_answering_queries(&mut session, DEFAULT_PROMPT)?;
    inspect_and_exit(&mut session, "LINE=<> POINT=<0>\r\n")
}
