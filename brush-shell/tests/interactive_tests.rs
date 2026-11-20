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

#[test_with::executable(sh, sleep)]
#[test]
fn resume_suspended_pipeline_in_foreground() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    // `fg` must continue every process of the pipeline, and give the terminal to their group,
    // or Ctrl+C can't end the pipeline.
    session.send_line("sleep 30 | sh -c 'echo ready-$((1 + 1)); exec sleep 31'")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending pipeline")?;

    // (`fg` shows the job's command once it has continued it.)
    session.send_line("fg")?;
    session.expect("exec sleep 31")?;
    session.interrupt()?;
    session
        .expect_prompt()
        .context("Prompt after interrupting the resumed pipeline")?;

    let jobs_output = session.exec_output("jobs")?;
    assert_eq!(jobs_output.trim(), "", "the pipeline should be gone");

    session.exit()?;

    Ok(())
}

#[test_with::executable(sh, sleep)]
#[test]
fn resume_suspended_pipeline_after_its_leader_exits() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    // The pipeline's first program leads its process group, which outlives it: `fg` must give
    // the terminal to the group, not to the first program left. (The second program waits for
    // the first to exit before it's ready.)
    session.send_line("sh -c 'exit 0' | sh -c 'cat; echo ready-$((1 + 1)); exec sleep 32'")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending pipeline")?;

    // (`fg` shows the job's command once it has continued it.)
    session.send_line("fg")?;
    session.expect("exec sleep 32")?;
    session.interrupt()?;
    session
        .expect_prompt()
        .context("Prompt after interrupting the resumed pipeline")?;

    let jobs_output = session.exec_output("jobs")?;
    assert_eq!(jobs_output.trim(), "", "the pipeline should be gone");

    session.exit()?;

    Ok(())
}

#[test_with::executable(sh, sleep, pgrep)]
#[test]
fn kill_suspended_pipeline() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    session.send_line("sleep 1017 | sh -c 'echo ready-$((1 + 1)); exec sleep 1018'")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending pipeline")?;

    // Killing the job kills all of its pipeline's processes.
    session.send_line("kill -9 %1; wait")?;
    session.expect_prompt()?;
    let jobs_output = session.exec_output("jobs")?;
    assert_eq!(jobs_output.trim(), "", "the job should be gone");
    assert_no_process_left(&mut session, "^sleep 101[78]$")?;

    session.exit()?;

    Ok(())
}

#[test_with::executable(sh, sleep, pgrep)]
#[test]
fn terminate_suspended_pipeline() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    session.send_line("sleep 1027 | sh -c 'echo ready-$((1 + 1)); exec sleep 1028'")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending pipeline")?;

    // A stopped job sent a request to terminate is continued, so that it acts on it.
    session.send_line("kill -TERM %1; wait")?;
    session.expect_prompt()?;
    let jobs_output = session.exec_output("jobs")?;
    assert_eq!(jobs_output.trim(), "", "the job should be gone");
    assert_no_process_left(&mut session, "^sleep 102[78]$")?;

    session.exit()?;

    Ok(())
}

#[test_with::executable(sh, head, sort)]
#[test]
fn resume_pipeline_stopped_in_background() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    // In the background, `head` reads from the terminal, which stops the pipeline; `fg` must
    // give it the terminal, and continue it, without its stopping again.
    session.send_line("sh -c 'echo ready-$((1 + 1)) >&2; exec head -n 2' | sort")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending pipeline")?;
    session.send_line("bg")?;
    session.expect_prompt()?;

    // (`fg` shows the job's command once it has continued it.)
    session.send_line("fg")?;
    session.expect("exec head -n 2")?;
    session.send("zebra\napple\n")?;
    session
        .expect(expectrl::Regex("apple\\s+zebra"))
        .context("sorted input after resuming the pipeline")?;
    session.expect_prompt()?;

    session.exit()?;

    Ok(())
}

#[test_with::executable(sh, seq, sed, less, stty, ps)]
#[test]
fn resume_pager_of_suspended_program() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;
    let shell_pid = session.exec_output("echo $$")?.trim().parse()?;
    let initial_modes = terminal_modes(&mut session)?;

    // A program that runs a pager (as `git log` does) is stopped along with it, and its other
    // processes, which `fg` must all continue, giving them the terminal.
    //
    // (The pager acts on Ctrl+Z only between reads from the terminal, by putting the terminal
    // back and stopping itself, so the test waits for it to be reading before suspending it,
    // and for it to have stopped before typing again.)
    session.send_line("sh -c 'seq 1 100 | sed s/^/line-/ | less'")?;
    session.expect("line-1")?;
    wait_for_state(shell_pid, "less", ProcessState::Sleeping)?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending the pager")?;
    wait_for_state(shell_pid, "less", ProcessState::Stopped)?;

    // The pager redraws and reads keys from the terminal once continued.
    session.send_line("fg")?;
    session.expect("line-1").context("pager redrawn")?;
    session.send("G")?;
    session.expect("line-100").context("pager's last line")?;

    // And it can be suspended and continued again.
    wait_for_state(shell_pid, "less", ProcessState::Sleeping)?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending the pager again")?;
    wait_for_state(shell_pid, "less", ProcessState::Stopped)?;
    session.send_line("fg")?;
    session.expect("line-100").context("pager redrawn again")?;
    session.send("q")?;
    session
        .expect_prompt()
        .context("Prompt after quitting the pager")?;

    // The pager leaves the terminal as it found it.
    assert_eq!(terminal_modes(&mut session)?, initial_modes);

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

/// A key bound with `bind -x` runs its command, which sees the line being edited.
#[test]
fn runs_commands_bound_to_keys() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    session.send_line(r#"bind -x '"\C-x": echo "bound:[$READLINE_LINE]"'"#)?;
    session.expect_prompt()?;

    // Type a line, and press Ctrl-X.
    session.send("abc\x18")?;
    session
        .expect("bound:[abc]")
        .context("bound command didn't run with the line")?;

    // The line comes back afterward; clear it with Ctrl-U before exiting.
    session.send("\x15")?;
    session.exit()?;

    Ok(())
}

/// Enter sends Ctrl-M, which the terminal passes on as is, so a key binding can
/// tell it from Ctrl-J (which is what `send_line` sends).
#[test]
fn tells_enter_from_ctrl_j() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    session.send_line(r#"bind -x '"\C-m": echo "enter:[$READLINE_LINE]"'"#)?;
    session.expect_prompt()?;

    session.send("abc\r")?;
    session
        .expect("enter:[abc]")
        .context("Enter didn't run the command bound to Ctrl-M")?;

    // The line comes back afterward; clear it with Ctrl-U before exiting.
    session.send("\x15")?;
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

#[test_with::executable(sh, sleep)]
#[test]
fn suspend_program_while_background_jobs_wait() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    // Background jobs wait for programs of their own while the foreground program runs; the
    // foreground program's stop is for its own wait alone. (Which wait hears of it first
    // varies, so more background jobs make for a likelier failure without the fix.)
    session.send_line("for i in {1..16}; do { sleep 5; } & done")?;
    session.expect_prompt()?;
    session.send_line("sh -c 'echo ready-$((1 + 1)); exec sleep 1061'")?;
    session.expect("ready-2")?;
    session.suspend()?;
    session
        .expect_prompt()
        .context("Prompt after suspending the program")?;

    let jobs_output = session.exec_output("jobs")?;
    assert!(
        jobs_output
            .lines()
            .any(|line| line.contains("Stopped") && line.contains("1061")),
        "unexpected jobs: {jobs_output}"
    );

    session.send_line("kill -9 %+")?;
    session.expect_prompt()?;

    session.exit()?;

    Ok(())
}

#[test_with::executable(sleep)]
#[test]
fn stop_signal_to_shell_leaves_background_jobs_alone() -> anyhow::Result<()> {
    let mut session = start_shell_session()?;
    session.expect_prompt()?;

    // A stop signal that reaches the shell stops none of the programs that background jobs are
    // waiting for, so none of them is taken to be stopped.
    session.send_line("{ sleep 3; echo bg-$((1 + 1)); } &")?;
    session.expect_prompt()?;
    // (The background job's program has started by the time the signal is sent.)
    let output =
        session.exec_output("sleep 0.5; kill -TSTP $$; sleep 1; echo marker-$((1 + 2))")?;
    assert!(output.contains("marker-3"), "unexpected output: {output}");
    assert!(
        !output.contains("Stopped") && !output.contains("bg-2"),
        "unexpected output: {output}"
    );

    session.exit()?;

    Ok(())
}

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

/// Asserts that no process at the session's terminal has a command line matching `pattern`.
fn assert_no_process_left(session: &mut ShellSession, pattern: &str) -> anyhow::Result<()> {
    // (macOS's `pgrep` has no `-s` to search the session, but has `-t`, which also finds
    // processes that aren't the shell's children. It exits with 1 when it finds nothing, and
    // with 2 or more when it fails.)
    let output = session.exec_output(format!(
        "t=$(tty); pgrep -f -t \"${{t#/dev/}}\" '{pattern}'; echo \"pgrep: $?\""
    ))?;
    assert_eq!(output.trim(), "pgrep: 1", "every process should be gone");
    Ok(())
}

/// A process's state, as `ps` reports it.
#[derive(Clone, Copy, Debug)]
enum ProcessState {
    /// Waiting, as for input (`S`).
    Sleeping,
    /// Stopped by a signal (`T`).
    Stopped,
}

/// Waits for the shell's descendants named `name` to be in the given state.
fn wait_for_state(shell_pid: u32, name: &str, state: ProcessState) -> anyhow::Result<()> {
    let state_code = match state {
        ProcessState::Sleeping => 'S',
        ProcessState::Stopped => 'T',
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ps = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=,ppid=,stat=,comm="])
            .output()?;
        let ps = String::from_utf8(ps.stdout)?;
        let processes: Vec<(u32, u32, &str, &str)> = ps
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((
                    fields.next()?.parse().ok()?,
                    fields.next()?.parse().ok()?,
                    fields.next()?,
                    fields.next()?,
                ))
            })
            .collect();

        let mut descendants = vec![shell_pid];
        let mut i = 0;
        while let Some(&parent) = descendants.get(i) {
            descendants.extend(processes.iter().filter(|p| p.1 == parent).map(|p| p.0));
            i += 1;
        }

        let named: Vec<_> = processes
            .iter()
            .filter(|p| descendants.contains(&p.0) && p.3.rsplit('/').next() == Some(name))
            .collect();
        if !named.is_empty() && named.iter().all(|p| p.2.starts_with(state_code)) {
            return Ok(());
        }

        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "{name} isn't {state:?}: {named:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Returns whether the session's terminal reads lines, echoes input, and generates signals.
fn terminal_modes(session: &mut ShellSession) -> anyhow::Result<Vec<String>> {
    let output = session.exec_output("stty -a")?;
    Ok(output
        .split(|c: char| c.is_whitespace() || c == ';')
        .filter(|mode| matches!(mode.trim_start_matches('-'), "icanon" | "echo" | "isig"))
        .map(str::to_owned)
        .collect())
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
