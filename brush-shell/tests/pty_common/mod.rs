//! Shared helpers for the pty-based test binaries in this directory.

#![allow(dead_code, reason = "each test binary uses a subset of these helpers")]

use std::time::Duration;

use expectrl::{
    Expect as _, Session,
    process::unix::{PtyStream, UnixProcess},
    stream::log::LogStream,
};

pub type PtySession = Session<UnixProcess, LogStream<PtyStream, std::io::Stdout>>;

pub const DEFAULT_PROMPT: &str = "brush> ";

/// Bound on how long we wait for the shell to respond. A healthy shell prompts in well
/// under a second; the generous margin only accommodates slow CI machines. Kept well
/// under any outer test-harness timeout so a hang fails here, with a specific message,
/// rather than stalling the harness.
const EXPECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Builds a hermetic brush invocation that, unlike the sessions in `interactive_tests.rs`,
/// leaves the input backend at its default (reedline/crossterm) — the backend real
/// terminal sessions use.
pub fn brush_command(extra_args: &[&str]) -> std::process::Command {
    let mut cmd = brush_command_with_bracketed_paste(extra_args);
    cmd.arg("--disable-bracketed-paste");
    cmd
}

/// Builds the same hermetic invocation, leaving bracketed paste enabled.
pub fn brush_command_with_bracketed_paste(extra_args: &[&str]) -> std::process::Command {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");

    let mut cmd = std::process::Command::new(shell_path);
    cmd.args(["--norc", "--noprofile", "--no-config", "--disable-color"]);
    cmd.args(extra_args);
    cmd.env("PS1", DEFAULT_PROMPT);
    cmd.env("TERM", "linux");

    cmd
}

/// Builds an invocation of the bash we compare against, or explains why there isn't one.
///
/// The compat suite gates oracle comparisons on a bash version; a pty test has to do the
/// same for itself, so that a machine with no bash (or one too old to have the behavior
/// under test) skips the comparison instead of failing it.
pub fn bash_oracle_command(
    min_version: (u32, u32),
) -> anyhow::Result<Option<std::process::Command>> {
    let Ok(bash_path) = which_bash() else {
        return Ok(None);
    };
    let version = brush_test_harness::util::get_bash_version_str(&bash_path)?;
    let mut parts = version.split('.').map(str::parse::<u32>);
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        anyhow::bail!("could not parse bash version: {version:?}");
    };
    if (major, minor) < min_version {
        return Ok(None);
    }

    let mut command = std::process::Command::new(bash_path);
    command.args(["--noprofile", "--norc", "-i"]);
    command.env("PS1", DEFAULT_PROMPT);
    command.env("TERM", "linux");
    command.env("INPUTRC", "/dev/null");

    Ok(Some(command))
}

fn which_bash() -> anyhow::Result<std::path::PathBuf> {
    let output = std::process::Command::new("sh")
        .args(["-c", "command -v bash"])
        .output()?;
    anyhow::ensure!(output.status.success(), "no bash on PATH");
    let path = String::from_utf8(output.stdout)?.trim().to_owned();
    anyhow::ensure!(!path.is_empty(), "no bash on PATH");

    Ok(path.into())
}

pub fn spawn_shell(cmd: std::process::Command) -> anyhow::Result<PtySession> {
    let session = Session::spawn(cmd)?;
    let mut session = expectrl::session::log(session, std::io::stdout())?;
    session.set_expect_timeout(Some(EXPECT_TIMEOUT));

    Ok(session)
}

/// Waits for `needle`, answering any cursor-position (DSR) queries the shell's terminal
/// backend emits along the way, as a real terminal emulator would. Without a response the
/// default input backend fails on its own query timeout, which would mask whatever this
/// test is actually trying to observe.
pub fn expect_answering_queries<N: expectrl::Needle + 'static>(
    session: &mut PtySession,
    needle: N,
) -> anyhow::Result<()> {
    const CURSOR_POSITION_QUERY: &str = "\x1b[6n";

    let needle = expectrl::Any::boxed(vec![Box::new(needle), Box::new(CURSOR_POSITION_QUERY)]);

    loop {
        let captures = session.expect(&needle)?;

        if captures.get(0) == Some(CURSOR_POSITION_QUERY.as_bytes()) {
            // Report the cursor as being at row 1, column 1.
            session.send("\x1b[1;1R")?;
        } else {
            return Ok(());
        }
    }
}
