//! tuish - A TUI-based interactive shell built on brush.

mod content_pane;
mod environment_pane;
mod ratatui_backend;
mod terminal_pane;

use std::collections::HashMap;
use std::sync::Arc;

use brush_builtins::ShellBuilderExt;
use brush_core::openfiles::OpenFile;
use brush_core::{ExecutionParameters, SourceInfo};
use ratatui_backend::RatatuiInputBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build the shell first (without PTY fds yet)
    let shell = brush_core::Shell::builder()
        .interactive(true)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .external_cmd_leads_session(true)
        .build()
        .await?;

    let shell = Arc::new(tokio::sync::Mutex::new(shell));

    // Create the ratatui TUI with shell reference
    let mut backend = RatatuiInputBackend::new(Arc::clone(&shell))?;

    // Now update shell with PTY fds from backend
    let fds = HashMap::from([
        (
            brush_core::openfiles::OpenFiles::STDIN_FD,
            OpenFile::File(backend.pty_stdin.try_clone()?),
        ),
        (
            brush_core::openfiles::OpenFiles::STDOUT_FD,
            OpenFile::File(backend.pty_stdout.try_clone()?),
        ),
        (
            brush_core::openfiles::OpenFiles::STDERR_FD,
            OpenFile::File(backend.pty_stderr.try_clone()?),
        ),
    ]);
    shell.lock().await.replace_open_files(fds.into_iter());

    // Run the main event loop
    run_event_loop(&mut backend, shell).await?;

    Ok(())
}

#[allow(clippy::unused_async)]
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
        match backend.handle_events()? {
            Some(command) if !command.is_empty() => {
                // User pressed Enter in command pane - execute the command
                let shell = Arc::clone(&shell);
                let source_info = source_info.clone();
                let params = params.clone();
                tokio::spawn(async move {
                    let result = {
                        let mut shell = shell.lock().await;
                        shell.run_string(command, &source_info, &params).await
                    };
                    let _ = result;
                });
            }
            Some(_) => {
                // Empty command, continue loop
            }
            None => {
                // None signals shutdown (Ctrl+Q was pressed)
                break;
            }
        }
    }

    Ok(())
}
