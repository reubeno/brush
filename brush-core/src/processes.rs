//! Process management

use futures::FutureExt;

use crate::{error, sys};

/// A waitable future that will yield the results of a child process's execution.
pub(crate) type WaitableChildProcess = std::pin::Pin<
    Box<dyn futures::Future<Output = Result<std::process::Output, std::io::Error>> + Send + Sync>,
>;

/// Tracks a child process being awaited.
pub struct ChildProcess {
    /// If available, the process ID of the child.
    pid: Option<sys::process::ProcessId>,
    /// A waitable future that will yield the results of a child process's execution.
    exec_future: WaitableChildProcess,
}

impl ChildProcess {
    /// Wraps a child process and its future.
    pub fn new(pid: Option<sys::process::ProcessId>, child: sys::process::Child) -> Self {
        Self {
            pid,
            exec_future: Box::pin(child.wait_with_output()),
        }
    }

    /// Returns the process's ID.
    pub const fn pid(&self) -> Option<sys::process::ProcessId> {
        self.pid
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ProcessWaitResult, error::Error> {
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigtstp = sys::signal::tstp_signal_listener()?;
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigchld = sys::signal::chld_signal_listener()?;

        #[allow(clippy::ignored_unit_patterns)]
        loop {
            tokio::select! {
                output = &mut self.exec_future => {
                    break Ok(ProcessWaitResult::Completed(output?))
                },
                _ = sigtstp.recv() => {
                    break Ok(ProcessWaitResult::Stopped)
                },
                _ = sigchld.recv() => {
                    if sys::signal::poll_for_stopped_children()? {
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

    pub(crate) fn poll(&mut self) -> Option<Result<std::process::Output, error::Error>> {
        let checkable_future = &mut self.exec_future;
        checkable_future
            .now_or_never()
            .map(|result| result.map_err(Into::into))
    }
}

/// Represents the result of waiting for an executing process.
pub enum ProcessWaitResult {
    /// The process completed.
    Completed(std::process::Output),
    /// The process stopped and has not yet completed.
    Stopped,
}
