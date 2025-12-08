//! tuish - A TUI-based interactive shell built on brush.

mod ratatui_backend;

use std::sync::Arc;

use brush_builtins::ShellBuilderExt;
use brush_interactive::{InteractiveShellExt, ShellRef};
use ratatui_backend::RatatuiInputBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the ratatui input backend
    let mut backend = RatatuiInputBackend::new()?;

    // Build the shell with PTY stdin/stdout/stderr
    // This ensures commands run in the PTY and their output appears in the terminal pane
    use brush_core::ShellFd;
    use brush_core::openfiles::OpenFile;
    use std::collections::HashMap;

    let mut fds = HashMap::new();
    fds.insert(
        ShellFd::from(0),
        OpenFile::File(backend.pty_stdin.try_clone()?),
    );
    fds.insert(
        ShellFd::from(1),
        OpenFile::File(backend.pty_stdout.try_clone()?),
    );
    fds.insert(
        ShellFd::from(2),
        OpenFile::File(backend.pty_stderr.try_clone()?),
    );

    let shell = brush_core::Shell::builder()
        .interactive(true)
        .fds(fds)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
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
