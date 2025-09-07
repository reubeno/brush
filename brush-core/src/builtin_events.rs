//! Emits notifications about builtin command execution.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::{OnceLock, atomic::{AtomicU64, Ordering}};
use tokio::sync::mpsc::UnboundedSender;

/// Describes a builtin-related event.
#[derive(Clone, Debug, Serialize)]
pub enum BuiltinEvent {
    /// A builtin was invoked.
    Spawn {
        /// Unique id for this builtin invocation.
        id: u64,
        /// Builtin name.
        name: String,
        /// Arguments passed to the builtin.
        args: Vec<String>,
        /// Working directory at invocation time.
        cwd: PathBuf,
    },
    /// A builtin completed.
    Exit {
        /// Unique id for this builtin invocation.
        id: u64,
        /// Exit code returned by the builtin.
        exit_code: i32,
    },
}

static BUILTIN_EVENT_SENDER: OnceLock<UnboundedSender<BuiltinEvent>> = OnceLock::new();
static BUILTIN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns a unique id for a builtin invocation.
pub fn next_id() -> u64 {
    BUILTIN_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Registers a sender that will receive [`BuiltinEvent`] notifications.
/// Subsequent calls have no effect once a sender has been registered.
pub fn set_builtin_event_sender(sender: UnboundedSender<BuiltinEvent>) {
    let _ = BUILTIN_EVENT_SENDER.set(sender);
}

/// Emits a [`BuiltinEvent`] to any registered subscribers.
pub fn emit(event: BuiltinEvent) {
    if let Some(sender) = BUILTIN_EVENT_SENDER.get() {
        let _ = sender.send(event);
    }
}

