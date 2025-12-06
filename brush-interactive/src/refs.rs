use std::sync::Arc;

use tokio::sync::Mutex;

/// A reference-counted, thread-safe reference to a `brush_core::Shell`.
pub type ShellRef = Arc<Mutex<brush_core::Shell>>;
