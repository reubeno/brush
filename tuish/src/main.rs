//! tuish - A TUI-based interactive shell built on brush.

mod app_ui;
mod command_input;
mod content_pane;
mod environment_pane;
mod pty;
mod terminal_pane;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use app_ui::AppUI;
use brush_builtins::ShellBuilderExt;
use brush_core::openfiles::OpenFile;
use environment_pane::EnvironmentPane;
use pty::Pty;
use terminal_pane::TerminalPane;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build the shell first (without PTY fds yet)
    let shell = brush_core::Shell::builder()
        .interactive(true)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .external_cmd_leads_session(true)
        .shell_name(String::from("tuish"))
        .shell_product_display_str(String::from("tuish"))
        .build()
        .await?;

    let shell = Arc::new(tokio::sync::Mutex::new(shell));

    // Create the ratatui TUI backend (empty, no panes yet)
    let mut ui = AppUI::new(&shell);

    // Calculate PTY dimensions based on UI layout and create the PTY.
    let (pty_rows, pty_cols) = ui.content_pane_dimensions()?;
    let pty = Pty::new(pty_rows, pty_cols)?;

    // Update shell with PTY fds
    let fds = HashMap::from([
        (
            brush_core::openfiles::OpenFiles::STDIN_FD,
            OpenFile::File(pty.stdin.try_clone()?),
        ),
        (
            brush_core::openfiles::OpenFiles::STDOUT_FD,
            OpenFile::File(pty.stdout.try_clone()?),
        ),
        (
            brush_core::openfiles::OpenFiles::STDERR_FD,
            OpenFile::File(pty.stderr.try_clone()?),
        ),
    ]);
    shell.lock().await.replace_open_files(fds.into_iter());

    // Create content panes
    let terminal_pane = Box::new(TerminalPane::new(pty.parser(), pty.writer()));
    let environment_pane = Box::new(EnvironmentPane::new(&shell));

    // Set the terminal pane (first in tab order, accessible for direct writes)
    ui.set_terminal_pane(terminal_pane);
    // Add other panes
    ui.add_pane(environment_pane);

    // Run the main event loop
    ui.run().await
}
