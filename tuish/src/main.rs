//! tuish - A TUI-based interactive shell built on brush.

mod aliases_pane;
mod app_ui;
mod callstack_pane;
mod command_input;
mod completion_pane;
mod content_pane;
mod environment_pane;
mod functions_pane;
mod history_pane;
mod layout;
mod pane_role;
mod pty;
mod region;
mod region_pane_store;
mod terminal_pane;

use std::collections::HashMap;
use std::sync::Arc;

use aliases_pane::AliasesPane;
use anyhow::Result;
use app_ui::AppUI;
use brush_builtins::ShellBuilderExt;
use brush_core::openfiles::OpenFile;
use callstack_pane::CallStackPane;
use completion_pane::CompletionPane;
use environment_pane::EnvironmentPane;
use functions_pane::FunctionsPane;
use history_pane::HistoryPane;
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

    // Calculate PTY dimensions based on UI layout and create the PTY.
    // We need to create the UI temporarily to get dimensions
    let temp_terminal = ratatui::init();
    let terminal_size = temp_terminal.size()?;
    ratatui::restore();

    // PTY dimensions: 80% of screen height for content area
    let content_height = (terminal_size.height * 80) / 100;
    let pty_rows = content_height
        .saturating_sub(1) // Tabs bar
        .saturating_sub(2); // Content border
    let pty_cols = terminal_size.width.saturating_sub(2); // Content left + right borders

    let pty = Arc::new(Pty::new(pty_rows, pty_cols)?);

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

    // Create special panes (terminal and completion)
    let terminal_pane = Box::new(TerminalPane::new(pty.parser(), pty.writer(), Arc::clone(&pty)));
    let completion_pane = Box::new(CompletionPane::new(&shell));

    // Create the UI with special panes
    let mut ui = AppUI::new(&shell, terminal_pane, completion_pane);

    // Add general content panes (roles no longer needed - IDs auto-assigned)
    ui.add_pane(Box::new(EnvironmentPane::new(&shell)));
    ui.add_pane(Box::new(HistoryPane::new(&shell)));
    ui.add_pane(Box::new(AliasesPane::new(&shell)));
    ui.add_pane(Box::new(FunctionsPane::new(&shell)));
    ui.add_pane(Box::new(CallStackPane::new(&shell)));

    // Run the main event loop
    ui.run().await
}
