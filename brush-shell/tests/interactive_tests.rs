//! Interactive integration tests for brush shell

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
use std::os::unix::process::CommandExt as _;

use expectrl::{
    Expect, Session,
    process::unix::{PtyStream, UnixProcess},
    repl::ReplSession,
    stream::log::LogStream,
};

#[test_with::executable(ping)]
#[test]
fn run_suspend_and_fg() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;

    // Ping localhost in a loop; wait for at least one response.
    session.expect_prompt()?;
    session.send_line("ping -c 1000000 127.0.0.1")?;
    session
        .expect("bytes from")
        .context("Initial ping invocation output")?;

    // Suspend and resume a handful of times to make sure it pauses and
    // resumes reliably.
    for _ in 0..5 {
        // Suspend.
        session.suspend()?;
        session.expect_prompt()?;

        // Run `jobs` to see the suspended job.
        let jobs_output = session.exec_output("jobs")?;
        assert!(jobs_output.contains("ping"));

        // Bring the job to the foreground.
        session.send_line("fg")?;
        session.expect("ping").context("Foregrounded ping")?;
    }

    // Ctrl+C to cancel the ping.
    session.interrupt()?;
    session.expect("loss")?;
    session.expect_prompt()?;

    // Exit the shell.
    session.exit()?;

    Ok(())
}

#[test_with::executable(ping)]
#[test]
fn run_in_bg_then_fg() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;

    // Ping localhost in a loop; wait for at least one response.
    session.expect_prompt()?;
    session.send_line("ping -c 1000000 127.0.0.1")?;
    session.expect("bytes from")?;

    // Suspend and send to background.
    session.suspend()?;
    session.expect_prompt()?;

    // Run `jobs` to see the suspended job.
    let jobs_output = session.exec_output("jobs")?;
    assert!(jobs_output.contains("ping"));

    // Send the job to the background.
    session.send_line("bg")?;
    session.expect_prompt()?;

    // Make sure ping is still running asynchronously.
    session.expect("bytes from")?;

    // Kill the job; make sure it's done.
    session.send_line("kill %1")?;
    session.expect_prompt()?;
    session.send_line("wait")?;
    session.expect_prompt()?;

    // Make sure the jobs are gone.
    let jobs_output = session.exec_output("jobs")?;
    assert_eq!(jobs_output.trim(), "");

    // Exit the shell.
    session.exit()?;

    Ok(())
}

#[test_with::executable(less)]
#[test]
fn run_pipeline_interactively() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;

    // Run a pipeline interactively.
    session.expect_prompt()?;
    session.send_line("echo hello | TERM=linux less")?;
    session
        .expect("hello")
        .context("Echoed text didn't show up")?;
    session.send("h")?;
    session
        .expect("SUMMARY")
        .context("less help didn't show up")?;
    session.send("q")?;
    session.send("q")?;
    session
        .expect_prompt()
        .context("Final prompt didn't show up")?;

    // Exit the shell.
    session.exit()?;

    Ok(())
}

#[test]
fn login_shell_via_argv0_shows_prompt() -> anyhow::Result<()> {
    let mut session = start_shell_session_with(|cmd| {
        cmd.arg("--norc").arg0("-brush");
    })?;

    session.expect_prompt()?;
    let login_shell_output = session.exec_output("shopt -q login_shell && echo login")?;
    assert!(login_shell_output.contains("login"));

    session.exit()?;

    Ok(())
}

#[test]
fn dash_s_with_positional_args_is_interactive() -> anyhow::Result<()> {
    // With `-s`, trailing words are positional parameters and commands still come from
    // stdin -- so at a terminal the shell is interactive, and `PROMPT_COMMAND` and the
    // zsh-style hooks (which are gated on that) run.
    let mut session = start_shell_session_with(|cmd| {
        cmd.args(["--norc", "-s", "myarg"]);
    })?;

    session.expect_prompt()?;

    // N.B. The markers are computed, so the echoed command line can't satisfy the assertion.
    let output = session.exec_output(r#"echo "FLAGS[${-//[!i]/}] ARG[$1]""#)?;
    assert!(
        output.contains("FLAGS[i] ARG[myarg]"),
        "expected an interactive shell with $1 set; got: {output}"
    );

    session.exec_output("PROMPT_COMMAND='echo PC-RAN'")?;
    let output = session.exec_output("echo done")?;
    assert!(
        output.contains("PC-RAN"),
        "PROMPT_COMMAND didn't run: {output}"
    );

    session.exit()?;

    Ok(())
}

#[test]
fn zsh_style_hook_state_is_visible_to_rc_files() -> anyhow::Result<()> {
    // The generic YAML harness always passes --norc, so this ordering check must use a PTY
    // session that can load a dedicated rc file.
    let rc_file = temp_script(
        r#"echo "RC-GUARD[${bash_preexec_imported:-unset}] RC-ARRAYS[${precmd_functions[*]:-unset}|${preexec_functions[*]:-unset}]"
rc_precmd() { echo RC-PRECMD; }
precmd_functions+=(rc_precmd)
"#,
    )?;

    let mut session = start_shell_session_with(|cmd| {
        cmd.arg("--enable-zsh-hooks")
            .arg("--rcfile")
            .arg(rc_file.path());
    })?;

    session.expect("RC-GUARD[defined] RC-ARRAYS[precmd|preexec]")?;
    session.expect("RC-PRECMD")?;
    session.expect_prompt()?;

    let output = session.exec_output("echo trigger")?;
    assert!(output.contains("RC-PRECMD"));

    session.exit()?;

    Ok(())
}

/// Terminal shell integration brackets each command with a matched pair of OSC 633 markers:
/// `E`/`C` before it runs, `D` after. A `preexec` hook that exits the shell means the command
/// never runs, so neither marker may be emitted -- a lone `D` desynchronizes a terminal's
/// command tracking.
#[test]
fn osc_command_markers_stay_paired_when_preexec_exits() -> anyhow::Result<()> {
    const COMMAND_STARTED: &str = "\x1b]633;C";
    const COMMAND_FINISHED: &str = "\x1b]633;D";

    let mut session = start_shell_session_with(|cmd| {
        cmd.args([
            "--norc",
            "--enable-zsh-hooks",
            "--enable-terminal-integration",
        ]);
        // OSC 633 is emitted only for terminals known to understand it.
        cmd.env("TERM_PROGRAM", "vscode");
    })?;
    session.expect_prompt()?;

    // A command that does run gets the full pair. (`preexec` isn't defined yet when this line
    // is dispatched, so the hook doesn't fire for it.)
    session.send_line("preexec() { case $1 in stop) exit 5;; esac; }")?;
    session
        .expect(COMMAND_STARTED)
        .context("no command-started marker for a command that ran")?;
    session
        .expect(COMMAND_FINISHED)
        .context("no command-finished marker for a command that ran")?;
    session.expect_prompt()?;

    // The hook exits before this line runs, so it gets neither marker.
    session.send_line("stop")?;
    // `Eof` matches the whole remaining buffer, so the bytes seen since the last prompt are
    // the match itself; `before()` would be empty.
    let captures = session
        .expect(expectrl::Eof)
        .context("shell did not exit")?;
    let tail = String::from_utf8_lossy(captures.as_bytes()).into_owned();

    assert!(
        !tail.contains(COMMAND_STARTED) && !tail.contains(COMMAND_FINISHED),
        "a command that never ran emitted markers: {tail:?}"
    );

    Ok(())
}

//
// Helpers
//

type ShellSession = ReplSession<Session<UnixProcess, LogStream<PtyStream, std::io::Stdout>>>;
// N.B. Comment out the above line and uncomment out the following line to disable logging of the
// session. type ShellSession = ReplSession<Session<UnixProcess, PtyStream>>;

trait SessionExt {
    fn suspend(&mut self) -> anyhow::Result<()>;
    fn interrupt(&mut self) -> anyhow::Result<()>;
    fn exec_output<S: AsRef<str>>(&mut self, cmd: S) -> anyhow::Result<String>;
}

impl SessionExt for ShellSession {
    fn suspend(&mut self) -> anyhow::Result<()> {
        // Send Ctrl+Z to suspend.
        self.send(expectrl::ControlCode::Substitute)?;
        Ok(())
    }

    fn interrupt(&mut self) -> anyhow::Result<()> {
        // Send Ctrl+C to interrupt.
        self.send(expectrl::ControlCode::EndOfText)?;
        Ok(())
    }

    fn exec_output<S: AsRef<str>>(&mut self, cmd: S) -> anyhow::Result<String> {
        let output = self.execute(cmd)?;
        let output_str = String::from_utf8(output)?;
        Ok(output_str)
    }
}

/// Writes `contents` to a temporary file that is deleted on drop.
fn temp_script(contents: &str) -> anyhow::Result<tempfile::NamedTempFile> {
    use std::io::Write as _;

    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(contents.as_bytes())?;
    Ok(file)
}

fn start_shell_session() -> anyhow::Result<ShellSession> {
    start_shell_session_with(|cmd| _ = cmd.arg("--norc"))
}

/// Starts a shell session at a pty. `configure` is the only place arguments and environment
/// come from beyond the fixed set below, so a session says for itself what it wants -- notably
/// `--norc`, or the `--rcfile` that a session wanting an rc file passes instead.
fn start_shell_session_with(
    configure: impl FnOnce(&mut std::process::Command),
) -> anyhow::Result<ShellSession> {
    const DEFAULT_PROMPT: &str = "brush> ";
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut cmd = std::process::Command::new(shell_path);
    cmd.args([
        "--noprofile",
        "--no-config",
        "--disable-bracketed-paste",
        "--disable-color",
        "--input-backend=basic",
    ]);
    cmd.env("PS1", DEFAULT_PROMPT);
    cmd.env("TERM", "linux");
    configure(&mut cmd);

    let session = expectrl::session::Session::spawn(cmd)?;

    // N.B. Comment out this line to disable logging of the session (along with a similar line
    // above).
    let session = expectrl::session::log(session, std::io::stdout())?;

    let mut session = expectrl::repl::ReplSession::new(session, DEFAULT_PROMPT);
    session.set_echo(true);

    Ok(session)
}
