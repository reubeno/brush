//! End-to-end tests for the bundled-command shim in `brush_shell::bundled`,
//! run against `brush-with-fakes`.

#![cfg(any(unix, windows))]
#![cfg(test)]

use std::process::{Command, Output, Stdio};

/// Runs `brush-with-fakes` with `args`, with nothing from the user's
/// configuration and no stdin.
#[allow(
    clippy::expect_used,
    reason = "a test helper; failing to run is a test failure"
)]
fn brush(args: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin!("brush-with-fakes"))
        .args(args)
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .stdin(Stdio::null())
        .output()
        .expect("running brush-with-fakes")
}

/// Runs `script` with `brush -c`.
fn run(script: &str) -> Output {
    brush(&["--norc", "--noprofile", "--no-config", "-c", script])
}

/// Checks that `script` writes exactly `stdout` and exits with `status`.
#[track_caller]
fn check(script: &str, stdout: &str, status: i32) {
    let output = run(script);
    assert_eq!(
        (
            String::from_utf8_lossy(&output.stdout).as_ref(),
            output.status.code()
        ),
        (stdout, Some(status)),
        "script: {script}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn arguments_pass_through_verbatim() {
    check(
        r"bundled-echo -x --help '' 'a b' -- --invoke-bundled '*'",
        "[bundled-echo]\n[-x]\n[--help]\n[]\n[a b]\n[--]\n[--invoke-bundled]\n[*]\n",
        0,
    );
    check("bundled-echo", "[bundled-echo]\n", 0);
    check(
        r#"set -- 'x y' ''; bundled-echo "$@" "$#""#,
        "[bundled-echo]\n[x y]\n[]\n[2]\n",
        0,
    );
}

#[test]
fn exit_status_is_the_command_s() {
    check("bundled-exit 42 2>/dev/null", "", 42);
    check(
        r#"bundled-exit 7 2>/dev/null; echo "status $?""#,
        "status 7\n",
        0,
    );
    check(
        "bundled-exit 0 2>/dev/null && echo yes; bundled-exit 1 2>/dev/null || echo no",
        "yes\nno\n",
        0,
    );
}

#[test]
fn redirections_apply() {
    check("bundled-exit 3 2>&1", "bundled-exit: 3\n", 3);
    check("bundled-echo hidden >/dev/null; echo done", "done\n", 0);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("out.txt");
    std::fs::write(&file, "stale contents\n").unwrap();
    let file = file.to_str().unwrap();
    let quoted = file.replace('\'', r"'\''");
    check(
        &format!("bundled-echo to file > '{quoted}'; bundled-echo again >> '{quoted}'"),
        "",
        0,
    );
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "[bundled-echo]\n[to]\n[file]\n[bundled-echo]\n[again]\n"
    );
}

#[test]
fn pipelines() {
    check(
        r#"bundled-echo one two | while read -r line; do echo "got $line"; done"#,
        "got [bundled-echo]\ngot [one]\ngot [two]\n",
        0,
    );
    check("echo piped | bundled-cat", "cat: piped\n", 0);
    check(
        "bundled-echo x | bundled-cat | bundled-cat",
        "cat: cat: [bundled-echo]\ncat: cat: [x]\n",
        0,
    );
    check(
        r#"echo x | bundled-exit 5 2>/dev/null; echo "status $?""#,
        "status 5\n",
        0,
    );
    check(
        r#"set -o pipefail; bundled-exit 6 2>/dev/null | bundled-cat; echo "status $?""#,
        "status 6\n",
        0,
    );
}

#[test]
fn stdin_from_here_strings_and_documents() {
    check("bundled-cat <<< 'here string'", "cat: here string\n", 0);
    check(
        "bundled-cat <<EOF\nline 1\nline 2\nEOF",
        "cat: line 1\ncat: line 2\n",
        0,
    );
}

#[test]
fn runs_wherever_a_builtin_runs() {
    check(
        r#"out=$(bundled-echo sub); echo "$out""#,
        "[bundled-echo]\n[sub]\n",
        0,
    );
    check("(bundled-echo sub)", "[bundled-echo]\n[sub]\n", 0);
    check(
        "command bundled-echo a; builtin bundled-echo b",
        "[bundled-echo]\n[a]\n[bundled-echo]\n[b]\n",
        0,
    );
}

#[test]
fn is_a_builtin_that_brush_s_own_builtins_win_over() {
    check("echo hi; type -t echo", "hi\nbuiltin\n", 0);
    check(
        "type bundled-echo; type -t bundled-echo; command -v bundled-echo",
        "bundled-echo is a shell builtin\nbuiltin\nbundled-echo\n",
        0,
    );

    let output = run("help bundled-echo");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("bundled-echo - bundled command"));
}

// Only Unix lets a parent choose a child's `argv[0]`.
#[cfg(unix)]
#[test]
fn the_child_s_argv0_is_the_bundled_name() {
    check("bundled-argv0", "bundled-argv0\n", 0);
}

#[test]
fn direct_dispatch() {
    #[track_caller]
    fn dispatch(args: &[&str], status: i32, stdout: &str, stderr: &str) {
        let output = brush(args);
        let got_stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(status), "args: {args:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            stdout,
            "args: {args:?}"
        );
        assert!(
            got_stderr.contains(stderr),
            "args: {args:?}\nstderr: {got_stderr}"
        );
    }

    dispatch(
        &["--invoke-bundled", "nope"],
        127,
        "",
        "unknown bundled command: nope",
    );
    dispatch(&["--invoke-bundled"], 2, "", "requires a command name");
    dispatch(
        &["--invoke-bundled", "bundled-exit", "4"],
        4,
        "",
        "bundled-exit: 4",
    );
    // Options after the name are the command's, not the shell's.
    dispatch(
        &["--invoke-bundled", "bundled-echo", "-c", "exit 9"],
        0,
        "[bundled-echo]\n[-c]\n[exit 9]\n",
        "",
    );
    // The flag dispatches only in first position.
    dispatch(
        &["-c", r#"echo "$0""#, "--invoke-bundled"],
        0,
        "--invoke-bundled\n",
        "",
    );
}
