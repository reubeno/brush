//! Process management

use std::{path::PathBuf, sync::PoisonError};

use futures::FutureExt;

use crate::{ExecutionResult, error, openfiles::OpenFiles, sys};

/// A waitable future that will yield the results of a child process's execution.
pub(crate) type WaitableChildProcess = std::pin::Pin<
    Box<dyn futures::Future<Output = Result<ExecutionResult, std::io::Error>> + Send>,
>;

/// Tracks a child process being awaited.
pub struct ChildProcess {
    /// A waitable future that will yield the results of a child process's execution.
    /// It's only ever reached through `&mut self` (with `get_mut`, which never locks); the
    /// mutex just makes this `Sync` without the future having to be.
    exec_future: std::sync::Mutex<WaitableChildProcess>,
    /// The process ID of the child, if it's a native process of the shell's.
    pid: Option<sys::process::ProcessId>,
    /// If available, the process group ID of the child.
    pgid: Option<sys::process::ProcessId>,
}

impl ChildProcess {
    /// Wraps a future that yields the result of a process, for a
    /// [`ProcessLauncher`](crate::extensions::ProcessLauncher) to return.
    ///
    /// # Arguments
    ///
    /// * `future` - The future that will yield the process's result.
    pub fn from_future(
        future: impl futures::Future<Output = Result<ExecutionResult, std::io::Error>> + Send + 'static,
    ) -> Self {
        Self {
            exec_future: std::sync::Mutex::new(Box::pin(future)),
            pid: None,
            pgid: None,
        }
    }

    /// Marks the process as a native child of the shell's, with the given IDs: job
    /// control signals it, and waiting for it notices when it stops.
    ///
    /// # Arguments
    ///
    /// * `pid` - The process ID, if known.
    /// * `pgid` - The process group ID, if the process has one.
    #[allow(dead_code, reason = "unused on platforms that can't start processes")]
    #[must_use]
    pub(crate) const fn native_child(
        mut self,
        pid: Option<sys::process::ProcessId>,
        pgid: Option<sys::process::ProcessId>,
    ) -> Self {
        self.pid = pid;
        self.pgid = pgid;
        self
    }

    /// Returns the process's ID, if it's a native process of the shell's.
    pub const fn pid(&self) -> Option<sys::process::ProcessId> {
        self.pid
    }

    /// Returns the process's group ID.
    pub const fn pgid(&self) -> Option<sys::process::ProcessId> {
        self.pgid
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ProcessWaitResult, error::Error> {
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigtstp = sys::signal::tstp_signal_listener()?;
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigchld = sys::signal::chld_signal_listener()?;

        // The child may have stopped before there were listeners to hear about it
        // (e.g., straight after `fg` resumed it).
        if let Some(pid) = self.pid
            && sys::signal::poll_for_stopped_child(pid)?
        {
            return Ok(ProcessWaitResult::Stopped);
        }

        #[allow(clippy::ignored_unit_patterns)]
        loop {
            tokio::select! {
                output = self.exec_future.get_mut().unwrap_or_else(PoisonError::into_inner) => {
                    break Ok(ProcessWaitResult::Completed(output?))
                },
                // Only this child's stop counts: another wait may be under way at the same time
                // (e.g., a background job's), for a child of its own, and the terminal's stop
                // may reach the shell without stopping this child.
                _ = sigtstp.recv() => {
                    if self.poll_for_stop()? {
                        break Ok(ProcessWaitResult::Stopped);
                    }
                },
                _ = sigchld.recv() => {
                    if self.poll_for_stop()? {
                        break Ok(ProcessWaitResult::Stopped);
                    }
                },
                _ = sys::signal::await_ctrl_c() => {
                    // SIGINT got thrown. Handle it and continue looping. The child should
                    // have received it as well, and either handled it or ended up getting
                    // terminated (in which case we'll see the child exit).
                },
            }
        }
    }

    /// Returns whether the process has stopped, consuming its stop status.
    fn poll_for_stop(&self) -> Result<bool, error::Error> {
        match self.pid {
            Some(pid) => sys::signal::poll_for_stopped_children(pid),
            None => Ok(false),
        }
    }

    pub(crate) fn poll(&mut self) -> Option<Result<ExecutionResult, error::Error>> {
        let checkable_future = self
            .exec_future
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        checkable_future
            .now_or_never()
            .map(|result| result.map_err(Into::into))
    }
}

/// Represents the result of waiting for an executing process.
pub enum ProcessWaitResult {
    /// The process completed.
    Completed(ExecutionResult),
    /// The process stopped and has not yet completed.
    Stopped,
}

/// A process for the shell to launch: a program the shell has already resolved
/// (builtins, functions, and the like never get this far), with everything it
/// should start with.
#[non_exhaustive]
pub struct LaunchRequest {
    /// Path to the executable, as resolved by the shell; relative to
    /// [`Self::working_dir`], if it's relative.
    pub program: PathBuf,
    /// The value the process should observe as `argv[0]`.
    pub argv0: String,
    /// Arguments to the process, not including `argv[0]`.
    pub args: Vec<String>,
    /// The complete environment for the process.
    pub env: Vec<(String, String)>,
    /// The working directory the process should start in.
    pub working_dir: PathBuf,
    /// The process's file descriptors. A standard stream (0, 1, or 2) that isn't
    /// among them is closed. The shell's own standard streams
    /// ([`OpenFile::Stdin`](crate::openfiles::OpenFile::Stdin), and so on) are
    /// shared with the process, under whichever descriptors hold them.
    pub open_files: OpenFiles,
    /// The process group the process should run in.
    pub process_group: ProcessGroup,
    /// Whether to kill the process if the shell drops the [`ChildProcess`] for it
    /// before it exits.
    pub kill_on_drop: bool,
}

/// The process group a process should run in. Platforms and
/// [`ProcessLauncher`](crate::extensions::ProcessLauncher)s without process
/// groups ignore this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProcessGroup {
    /// The shell's process group.
    Inherit,
    /// An existing process group, with the given ID.
    Join(sys::process::ProcessId),
    /// A new process group, led by the process.
    New {
        /// Whether the new group should become the terminal's foreground group.
        take_foreground: bool,
    },
    /// A new session, led by the process, with the terminal as its controlling
    /// terminal.
    NewSession,
}
