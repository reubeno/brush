//! tuish - A TUI-based interactive shell built on brush.

mod ratatui_backend;

use std::collections::HashMap;
use std::sync::Arc;

use brush_builtins::ShellBuilderExt;
use brush_core::openfiles::OpenFile;
use brush_core::{ExecutionParameters, ShellFd, SourceInfo};
use ratatui_backend::RatatuiInputBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the ratatui TUI
    let mut backend = RatatuiInputBackend::new()?;

    // Build the shell with PTY stdin/stdout/stderr
    // This ensures commands run in the PTY and their output appears in the terminal pane
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
        .external_cmd_leads_session(true)
        .build()
        .await?;

    let shell = Arc::new(tokio::sync::Mutex::new(shell));

    // Run the main event loop
    run_event_loop(&mut backend, shell).await?;

    Ok(())
}

async fn run_event_loop(
    backend: &mut RatatuiInputBackend,
    shell: Arc<tokio::sync::Mutex<brush_core::Shell>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_info = SourceInfo::default();
    let params = ExecutionParameters::default();

    loop {
        // Render the UI
        backend.draw_ui()?;

        // Handle events (keyboard input, etc.) with 16ms timeout (~60 FPS)
        if let Some(command) = backend.handle_events()? {
            // User pressed Enter in command pane - execute the command
            let shell = Arc::clone(&shell);
            let source_info = source_info.clone();
            let params = params.clone();
            tokio::spawn(async move {
                let mut shell = shell.lock().await;
                let _ = shell.run_string(command, &source_info, &params).await;
            });
        }
    }
}
