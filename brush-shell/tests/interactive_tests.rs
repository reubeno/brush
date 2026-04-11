//! Interactive integration tests for brush shell

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
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

fn start_shell_session() -> anyhow::Result<ShellSession> {
    const DEFAULT_PROMPT: &str = "brush> ";
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut cmd = std::process::Command::new(shell_path);
    cmd.args([
        "--norc",
        "--noprofile",
        "--no-config",
        "--disable-bracketed-paste",
        "--disable-color",
        "--input-backend=basic",
    ]);
    cmd.env("PS1", DEFAULT_PROMPT);
    cmd.env("TERM", "linux");

    let session = expectrl::session::Session::spawn(cmd)?;

    // N.B. Comment out this line to disable logging of the session (along with a similar line
    // above).
    let session = expectrl::session::log(session, std::io::stdout())?;

    let mut session = expectrl::repl::ReplSession::new(session, DEFAULT_PROMPT);
    session.set_echo(true);

    Ok(session)
}

// --- Regression tests for docs/todo/login-hang.md ---

/// Regression test for H2: if brush is started from a parent shell that does *not*
/// put brush into its own process group (i.e. bash with `set +m`), brush inherits
/// the parent's pgid. `TerminalControl::acquire()` then runs `setpgid(0,0)` and
/// `tcsetpgrp(...)`. Historically two bugs made this hang:
///
///   1. The `TerminalControl` guard was dropped immediately because the result of
///      `acquire()?` was unbound, so the previous fg pgid got tcsetpgrp'd right back
///      and brush was left in a background pg.
///   2. `mask_sigttou()` was called *after* `move_self_to_foreground`, so the
///      tcsetpgrp inside acquire itself ran from a background pg and was interrupted
///      by the default SIGTTOU (stop) action.
///
/// Symptom: brush stops with state=T before ever printing a prompt.
///
/// Repro shape: `bash -c 'set +m; brush ...; true'`. `set +m` disables job control in
/// bash so brush inherits bash's pgid; the trailing `; true` prevents bash's -c
/// single-command exec optimization so brush is actually forked (not exec-replaced).
#[test]
fn login_hang_non_pgleader_pty() -> anyhow::Result<()> {
    const BRUSH_PROMPT: &str = "brush> ";
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    // Wrap brush inside a bash subshell with job control disabled. PS1 must be
    // *exported* from the wrapper for brush to pick it up (bash imports PS1 as a
    // non-exported local by default). The trailing `; true` prevents bash's -c
    // single-command exec optimization so brush is actually forked (not
    // exec-replaced) and thus inherits bash's pgid rather than running with its
    // own.
    let brush_invocation = format!(
        "{} --norc --noprofile --no-config --disable-bracketed-paste \
         --disable-color --input-backend=basic",
        shell_path.to_string_lossy()
    );
    let wrapper_script = format!("export PS1='{BRUSH_PROMPT}'; set +m; {brush_invocation}; true");

    let mut cmd = std::process::Command::new("bash");
    cmd.args(["--norc", "--noprofile", "-c", &wrapper_script]);
    cmd.env("TERM", "linux");

    let mut session = expectrl::session::Session::spawn(cmd)?;
    // If brush hangs (SIGTTIN'd into T state), the prompt will never arrive. Fail
    // fast rather than hanging the test run.
    session.set_expect_timeout(Some(std::time::Duration::from_secs(5)));

    session
        .expect(BRUSH_PROMPT)
        .context("brush prompt never arrived — likely hung on SIGTTIN/SIGTTOU")?;

    // Make sure brush can actually handle input in this configuration.
    session.send_line("echo alive-and-well")?;
    session
        .expect("alive-and-well")
        .context("brush failed to execute a command after reaching prompt")?;
    session.send_line("exit")?;

    Ok(())
}
