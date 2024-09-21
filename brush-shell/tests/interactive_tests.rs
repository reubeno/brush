#![allow(clippy::panic_in_result_fn)]

use expectrl::{
    process::unix::{PtyStream, UnixProcess},
    repl::ReplSession,
    Expect, Session,
};

#[ignore] // TODO: Debug flakiness in GitHub runs.
#[test]
fn run_suspend_and_fg() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;

    // Ping localhost in a loop; wait for at least one response.
    session.expect_prompt()?;
    session.send_line("ping 127.0.0.1")?;
    session.expect("bytes from")?;

    // Suspend and resume a handful of times to make sure it pauses and
    // resumes reliably.
    for _ in 0..5 {
        // Suspend.
        session.suspend()?;
        session.expect_prompt()?;

        // Run `jobs` to see the suspended job.
        let jobs_output = session.exec_output("jobs")?;
        assert!(jobs_output.contains("ping 127.0.0.1"));

        // Bring the job to the foreground.
        session.send_line("fg")?;
        session.expect("ping")?;
    }

    // Ctrl+C to cancel the ping.
    session.interrupt()?;
    session.expect("loss")?;
    session.expect_prompt()?;

    // Exit the shell.
    session.exit()?;

    Ok(())
}

#[ignore] // TODO: Debug flakiness in GitHub runs.
#[test]
fn run_in_bg_then_fg() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;

    // Ping localhost in a loop; wait for at least one response.
    session.expect_prompt()?;
    session.send_line("ping 127.0.0.1")?;
    session.expect("bytes from")?;

    // Suspend and send to background.
    session.suspend()?;
    session.expect_prompt()?;

    // Run `jobs` to see the suspended job.
    let jobs_output = session.exec_output("jobs")?;
    assert!(jobs_output.contains("ping 127.0.0.1"));

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

//
// Helpers
//

// N.B. Uncomment the following line to enable logging of the session (along with a similar line
// below). type ShellSession = ReplSession<Session<UnixProcess, LogStream<PtyStream,
// std::io::Stdout>>>;
type ShellSession = ReplSession<Session<UnixProcess, PtyStream>>;

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
    cmd.args(["--norc", "--noprofile", "--disable-bracketed-paste"]);
    cmd.env("PS1", DEFAULT_PROMPT);
    cmd.env("TERM", "dumb");

    let session = expectrl::session::Session::spawn(cmd)?;

    // N.B. Uncomment this line to enable logging of the session (along with a similar line above).
    // let session = expectrl::session::log(session, std::io::stdout())?;

    let session = expectrl::repl::ReplSession::new(session, DEFAULT_PROMPT);

    Ok(session)
}
