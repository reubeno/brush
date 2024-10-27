//! Interactive integration tests for brush shell

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]

use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        mpsc::{Receiver, Sender, TryRecvError},
        Arc,
    },
};

use anyhow::Context;
use expectrl::{
    process::{
        unix::{PtyStream, UnixProcess},
        NonBlocking,
    },
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
    let mut session = start_shell_session_with_alacritty()?;

    // Run a pipeline interactively.
    session.expect_prompt()?;

    // NOTE: `send_line` and `\n` don't work with the `reedline` backend.
    session.send("echo hello | LESS= less -X\r\n")?;
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

// alacritty session
type AlacrittyShellSession =
    ReplSession<Session<SessionProcess, LogStream<SessionStream, std::io::Stdout>>>;
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

use alacritty_terminal::{event::EventListener, event_loop::Notifier};
use alacritty_terminal::{
    event::{Event, Notify, WindowSize},
    event_loop::EventLoop,
    sync::FairMutex,
    term::{test::TermSize, Config},
    tty::{self, Options, Shell},
    Term,
};

// handle callbacks from the terminal such as ChildExit, PtyWrite etc.
#[derive(Clone)]
pub struct AlacrittyEventListener(pub Sender<Event>);

impl EventListener for AlacrittyEventListener {
    fn send_event(&self, event: Event) {
        self.0.send(event).ok();
    }
}

// alacritty will duplicate all its text here
struct AlacrittyRecorder(std::sync::mpsc::Sender<Vec<u8>>);

impl Write for AlacrittyRecorder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.send(buf.into()).unwrap();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct SessionStream {
    pty_tx: Notifier,
    alacritty_recording_rx: Receiver<Vec<u8>>,
    non_blocking: bool,
    buf: Vec<u8>,
}

impl SessionStream {
    fn new(pty_tx: Notifier, alacritty_recording_rx: Receiver<Vec<u8>>) -> Self {
        SessionStream {
            pty_tx,
            alacritty_recording_rx,
            non_blocking: true,
            buf: Vec::new(),
        }
    }
}
// write to the shell stdin through pty
impl Write for SessionStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pty_tx.notify(buf.to_owned());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for SessionStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: `non_blocking = false` is not used, maybe it does not necessary
        match self.alacritty_recording_rx.try_recv() {
            Ok(l) => {
                self.buf.extend(l.iter());
                let len = std::cmp::min(self.buf.len(), buf.len());
                buf[0..len].clone_from_slice(&self.buf.drain(0..len).as_slice());
                Ok(len)
            }
            // EOF
            Err(TryRecvError::Disconnected) => Ok(0),
            // We are not ready yet
            Err(TryRecvError::Empty) => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

impl NonBlocking for SessionStream {
    fn set_blocking(&mut self, on: bool) -> std::io::Result<()> {
        self.non_blocking = on;
        Ok(())
    }
}

// TODO: maybe some data should be stored in Session process?
struct SessionProcess {}

fn start_shell_session_with_alacritty() -> anyhow::Result<AlacrittyShellSession> {
    const DEFAULT_PROMPT: &str = "brush> ";
    alacritty_terminal::tty::setup_env();
    let shell_path = assert_cmd::cargo::cargo_bin("brush");
    let shell_path = String::from_utf8_lossy(shell_path.as_os_str().as_encoded_bytes());
    let shell = Some(Shell::new(
        shell_path.to_string(),
        [
            "--norc",
            "--noprofile",
            "--disable-bracketed-paste",
            "--disable-color",
            // "--input-backend=basic",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect(),
    ));

    let options = Options {
        working_directory: None,
        shell,
        hold: false,
        env: [("TERM", "xterm-256color"), ("PS1", DEFAULT_PROMPT)]
            .iter()
            .map(|e| (e.0.to_string(), e.1.to_string()))
            .collect::<HashMap<_, _>>(),
    };
    let size = TermSize::new(80, 10);
    let config = Config::default();

    let (events_tx, events_rx) = std::sync::mpsc::channel();

    let term = Term::new(config, &size, AlacrittyEventListener(events_tx.clone()));
    let term = Arc::new(FairMutex::new(term));

    let size = WindowSize {
        num_lines: 10,
        num_cols: 80,
        cell_width: 1,
        cell_height: 1,
    };

    let pty = tty::new(&options, size.into(), 1u64)?;

    let event_loop = EventLoop::new(
        Arc::clone(&term),
        AlacrittyEventListener(events_tx.clone()),
        pty,
        options.hold,
        true,
    )?;

    let pty_tx = event_loop.channel();
    let notif = Notifier(pty_tx.clone());
    let pty_tx = Notifier(pty_tx);

    let (tx, al_buf_rx) = std::sync::mpsc::channel();

    let _io_thread = event_loop.spawn_with_pipe(Some(AlacrittyRecorder(tx)));

    std::thread::spawn(move || {
        'ev_loop: while let Ok(ev) = events_rx.recv() {
            match ev {
                // write terminal response, for example response to the cursor position request
                // back to the pty
                Event::PtyWrite(out) => {
                    notif.notify(out.into_bytes());
                }
                Event::ChildExit(_exit_code) => break 'ev_loop,
                Event::Wakeup => {}
                _ => {}
            }
        }
    });
    let p = SessionProcess {};
    let session = expectrl::session::Session::new(p, SessionStream::new(pty_tx, al_buf_rx))?;
    let session = expectrl::session::log(session, std::io::stdout())?;

    let session = expectrl::repl::ReplSession::new(session, DEFAULT_PROMPT);
    Ok(session)
}
