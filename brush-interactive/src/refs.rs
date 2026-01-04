use std::sync::Arc;

use tokio::sync::Mutex;

/// A reference-counted, thread-safe reference to a `brush_core::ShellRuntime` implementation.
pub type ShellRef<S: brush_core::ShellRuntime> = Arc<Mutex<S>>;
