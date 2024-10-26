//! Interactive integration tests for brush shell

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use expectrl::{
    process::unix::{PtyStream, UnixProcess},
    repl::ReplSession,
    stream::log::LogStream,
    Expect, Session,
};

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
    assert!(jobs_output.trim().is_empty());

    // Exit the shell.
    session.exit()?;

    Ok(())
}

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
    let shell_path = assert_cmd::cargo::cargo_bin("brush");

    let mut cmd = std::process::Command::new(shell_path);
    cmd.args([
        "--norc",
        "--noprofile",
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

    let session = expectrl::repl::ReplSession::new(session, DEFAULT_PROMPT);

    Ok(session)
}
