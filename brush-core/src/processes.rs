//! Process management

use futures::FutureExt;

use crate::{ExecutionParameters, error, sys, traps};

/// A waitable future that will yield the results of a child process's execution.
pub(crate) type WaitableChildProcess = std::pin::Pin<
    Box<dyn futures::Future<Output = Result<std::process::Output, std::io::Error>> + Send + Sync>,
>;

/// Tracks a child process being awaited.
pub struct ChildProcess {
    /// A waitable future that will yield the results of a child process's execution.
    exec_future: WaitableChildProcess,
    /// If available, the process ID of the child.
    pid: Option<sys::process::ProcessId>,
    /// If available, the process group ID of the child.
    pgid: Option<sys::process::ProcessId>,
    /// Whether a host-requested signal was forwarded while waiting.
    forwarded_signal: bool,
}

impl ChildProcess {
    /// Wraps a child process and its future.
    pub fn new(
        child: sys::process::Child,
        pid: Option<sys::process::ProcessId>,
        pgid: Option<sys::process::ProcessId>,
    ) -> Self {
        Self {
            exec_future: Box::pin(child.wait_with_output()),
            pid,
            pgid,
            forwarded_signal: false,
        }
    }

    /// Returns the process's ID.
    pub const fn pid(&self) -> Option<sys::process::ProcessId> {
        self.pid
    }

    /// Returns the process's group ID.
    pub const fn pgid(&self) -> Option<sys::process::ProcessId> {
        self.pgid
    }

    /// Returns whether a host-requested signal was forwarded to this child.
    pub const fn forwarded_signal(&self) -> bool {
        self.forwarded_signal
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ProcessWaitResult, error::Error> {
        self.wait_impl(None).await
    }

    pub(crate) async fn wait_with_params(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<ProcessWaitResult, error::Error> {
        self.wait_impl(Some(params)).await
    }

    async fn wait_impl(
        &mut self,
        params: Option<&ExecutionParameters>,
    ) -> Result<ProcessWaitResult, error::Error> {
        let mut observed_signal_generation = params
            .map(ExecutionParameters::pending_signal_generation)
            .unwrap_or_default();

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
                (signal_generation, signal_number, forward_to_child) = wait_for_pending_signal(params, observed_signal_generation), if params.is_some() => {
                    observed_signal_generation = signal_generation;
                    if forward_to_child {
                        self.forward_signal(signal_number);
                    } else if let Some(params) = params {
                        params.clear_pending_signal();
                    }
                },
            }
        }
    }

    fn forward_signal(&mut self, signal_number: i32) {
        let Ok(signal) = sys::signal::Signal::try_from(signal_number) else {
            return;
        };

        self.forwarded_signal = true;
        let signal = traps::TrapSignal::Signal(signal);
        if let Some(pgid) = self.pgid {
            if let Err(error) = sys::signal::kill_process_group(pgid, signal) {
                tracing::debug!(
                    pgid,
                    %error,
                    "failed to forward pending signal to child process group"
                );
            }
            return;
        }

        if let Some(pid) = self.pid
            && let Err(error) = sys::signal::kill_process(pid, signal)
        {
            tracing::debug!(pid, %error, "failed to forward pending signal to child process");
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

async fn wait_for_pending_signal(
    params: Option<&ExecutionParameters>,
    observed_generation: u64,
) -> (u64, i32, bool) {
    let Some(params) = params else {
        futures::future::pending().await
    };

    params
        .wait_for_pending_signal_after(observed_generation)
        .await
}
