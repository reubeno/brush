use std::sync::Arc;

use tokio::sync::Mutex;

/// A reference-counted, thread-safe reference to a `brush_core::Shell`.
#[allow(type_alias_bounds)]
pub type ShellRef<
    SE: brush_core::ShellExtensions = brush_core::extensions::DefaultShellExtensions,
> = Arc<Mutex<brush_core::Shell<SE>>>;
