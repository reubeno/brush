//! Process management utilities

pub(crate) type ProcessId = i32;

/// Provides access to a child process.
pub struct Child {
    inner: std::process::Child,
}

pub(crate) use std::process::ExitStatus;
pub(crate) use std::process::Output;

impl Child {
    /// Returns the process ID of the child process, if available.
    pub fn id(&self) -> Option<u32> {
        None
    }

    /// Asynchronously waits for the child process to exit.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.inner.wait()
    }

    /// Asynchronously waits for the child process to exit and collects its
    /// output.
    pub async fn wait_with_output(self) -> std::io::Result<Output> {
        self.inner.wait_with_output()
    }
}

pub(crate) fn spawn(
    mut command: std::process::Command,
    kill_on_drop: bool,
) -> std::io::Result<Child> {
    // No kill-on-drop on this stub platform; accepted and ignored.
    let _ = kill_on_drop;
    let child = command.spawn()?;
    Ok(Child { inner: child })
}
