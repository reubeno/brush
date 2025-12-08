//! tuish - A TUI-based interactive shell built on brush.

mod ratatui_backend;

use std::sync::Arc;

use brush_interactive::{InteractiveShellExt, ShellRef};
use ratatui_backend::RatatuiInputBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the ratatui input backend
    let mut backend = RatatuiInputBackend::new()?;

    // Build the shell - it will use standard stdin/stdout/stderr by default
    // The PTY integration happens transparently through the backend
    let shell = brush_core::Shell::builder()
        .interactive(true)
        .build()
        .await?;

    let shell = Arc::new(tokio::sync::Mutex::new(shell));
    let shell_ref = ShellRef::from(Arc::clone(&shell));

    // Run the interactive shell loop
    let result = shell_ref.run_interactively(&mut backend).await;

    // Clean up
    drop(backend);

    result.map_err(Into::into)
}
