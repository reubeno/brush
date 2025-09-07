//! Emits notifications about external process execution.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::mpsc::UnboundedSender;

/// Describes a process-related event.
#[derive(Clone, Debug, Serialize)]
pub enum ProcessEvent {
    /// A process was spawned.
    Spawn {
        /// Parent process identifier.
        ppid: i32,
        /// Process identifier of the spawned child.
        pid: i32,
        /// Path of the command that was executed.
        command: String,
        /// Arguments passed to the command.
        args: Vec<String>,
        /// Working directory used when spawning the process.
        cwd: PathBuf,
    },
    /// A process exited.
    Exit {
        /// Process identifier.
        pid: i32,
        /// Exit code returned by the process.
        exit_code: i32,
    },
}

static PROCESS_EVENT_SENDER: OnceLock<UnboundedSender<ProcessEvent>> = OnceLock::new();

/// Registers a sender that will receive [`ProcessEvent`] notifications.
///
/// Subsequent calls have no effect once a sender has been registered.
pub fn set_process_event_sender(sender: UnboundedSender<ProcessEvent>) {
    let _ = PROCESS_EVENT_SENDER.set(sender);
}

pub(crate) fn emit(event: ProcessEvent) {
    if let Some(sender) = PROCESS_EVENT_SENDER.get() {
        let _ = sender.send(event);
    }
}
