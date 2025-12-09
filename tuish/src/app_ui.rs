//! Ratatui-based TUI for tuish shell.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use brush_core::{ExecutionParameters, SourceInfo};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Tabs},
};
use tokio::sync::Mutex;

use crate::command_input::CommandInput;
use crate::content_pane::ContentPane;

/// Which area currently has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedArea {
    /// A content pane is focused (index of the pane)
    Pane(usize),
    /// Command input area is focused
    CommandInput,
}

/// Ratatui-based TUI that displays content panes in tabs
/// and accepts command input in a bottom pane.
pub struct AppUI {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// The shell instance
    shell: Arc<Mutex<brush_core::Shell>>,
    /// Content panes displayed in tabs
    panes: Vec<Box<dyn ContentPane>>,
    /// Command input widget
    command_input: CommandInput,
    /// Which area currently has focus
    focused_area: FocusedArea,
}

impl AppUI {
    /// Creates a new `AppUI` without any content panes.
    ///
    /// Use `add_pane` to add content panes after construction.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to run the UI for.
    pub fn new(shell: &Arc<Mutex<brush_core::Shell>>) -> Self {
        // Initialize the ratatui terminal in raw mode
        let terminal = ratatui::init();

        Self {
            terminal,
            shell: shell.clone(),
            panes: Vec::new(),
            command_input: CommandInput::new(shell),
            focused_area: FocusedArea::CommandInput,
        }
    }

    /// Adds a content pane to the backend.
    ///
    /// Panes are displayed in tabs in the order they are added.
    pub fn add_pane(&mut self, pane: Box<dyn ContentPane>) {
        self.panes.push(pane);
    }

    /// Returns the current terminal size.
    pub fn terminal_size(&self) -> Result<ratatui::layout::Size, std::io::Error> {
        self.terminal.size()
    }

    /// Calculates the appropriate dimensions for a content pane that will be displayed
    /// in the tabbed area.
    ///
    /// This accounts for:
    /// - The tab area taking 80% of screen height
    /// - Tab bar (1 line)
    /// - Content borders (2 lines)
    /// - tui-term quirk (doesn't use last row, so subtract 1 more)
    /// - Content left + right borders (2 columns)
    ///
    /// # Returns
    /// A tuple of (rows, cols) suitable for PTY or other content pane sizing
    ///
    /// # Errors
    /// Returns an error if terminal size cannot be determined
    pub fn content_pane_dimensions(&self) -> Result<(u16, u16), std::io::Error> {
        let terminal_size = self.terminal_size()?;

        // PTY dimensions: The top area (80% of screen) contains tabs + bordered content.
        let terminal_pane_height = (terminal_size.height * 80) / 100;
        let rows = terminal_pane_height
            .saturating_sub(1) // Tabs bar
            .saturating_sub(2) // Content border
            .saturating_sub(1); // tui-term quirk
        let cols = terminal_size.width.saturating_sub(2); // Content left + right borders

        Ok((rows, cols))
    }

    /// Gets mutable access to a specific pane.
    #[allow(dead_code)]
    pub fn get_pane_mut(&mut self, index: usize) -> Option<&mut dyn ContentPane> {
        if let Some(boxed_pane) = self.panes.get_mut(index) {
            Some(boxed_pane.as_mut())
        } else {
            None
        }
    }

    /// Returns the currently selected pane index.
    #[allow(dead_code)]
    pub const fn selected_pane(&self) -> Option<usize> {
        match self.focused_area {
            FocusedArea::Pane(index) => Some(index),
            FocusedArea::CommandInput => None,
        }
    }

    /// Draws the UI with content panes and command input.
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        let focused_area = self.focused_area;

        // Collect tab titles from panes
        let tab_titles: Vec<String> = self.panes.iter().map(|p| p.name().to_string()).collect();

        // Track which pane is selected (for rendering) vs focused (for input)
        let selected_pane_index = match focused_area {
            FocusedArea::Pane(idx) => idx,
            FocusedArea::CommandInput => 0, // Keep showing first pane when command input is focused
        };

        // Update command input focus state
        self.command_input
            .set_focused(matches!(focused_area, FocusedArea::CommandInput));

        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(80), // Tab area (tabs + content)
                    Constraint::Percentage(20), // Command input pane
                ])
                .split(f.area());

            // Split tab area into: tabs bar (1 line) + content
            let tab_area_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Tabs bar (just the labels, no borders)
                    Constraint::Min(0),    // Content area with borders
                ])
                .split(chunks[0]);

            // Render the tabs (outside any borders)
            // Deselect all tabs when command input is focused
            let tab_selection = match focused_area {
                FocusedArea::Pane(idx) => idx,
                FocusedArea::CommandInput => usize::MAX, // Deselect when command input focused
            };

            let tabs = Tabs::new(tab_titles.iter().map(|t| Line::from(t.as_str())))
                .select(tab_selection)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray)) // Unselected tabs
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .divider(" │ ")
                .padding(" ", " ");
            f.render_widget(tabs, tab_area_chunks[0]);

            // Render content area with borders based on focus
            let pane_focused = matches!(focused_area, FocusedArea::Pane(_));
            let content_border_style = if pane_focused {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Render the selected pane's content with borders
            let content_block = Block::default()
                .borders(Borders::ALL)
                .border_style(content_border_style);
            let content_inner = content_block.inner(tab_area_chunks[1]);
            f.render_widget(content_block, tab_area_chunks[1]);

            // Render the selected pane's content inside the bordered area
            if selected_pane_index < self.panes.len() {
                self.panes[selected_pane_index].render(f, content_inner);
            }

            // Render the command input pane using the widget
            if let Some((cursor_x, cursor_y)) = self.command_input.render(f, chunks[1]) {
                f.set_cursor_position((cursor_x, cursor_y));
            }
        })?;

        Ok(())
    }

    /// Handles keyboard input and returns Some(command) when Enter is pressed in command pane.
    /// Returns None to signal application shutdown (Ctrl+Q).
    #[allow(clippy::too_many_lines)]
    pub fn handle_events(&mut self) -> Result<Option<String>, std::io::Error> {
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    // Ctrl+Space cycles focus through panes and command input
                    KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let num_panes = self.panes.len();
                        self.focused_area = match self.focused_area {
                            FocusedArea::Pane(idx) if idx + 1 < num_panes => {
                                FocusedArea::Pane(idx + 1)
                            }
                            FocusedArea::Pane(_) => FocusedArea::CommandInput,
                            FocusedArea::CommandInput => FocusedArea::Pane(0),
                        };
                    }
                    // Ctrl+Q quits the application by returning None to signal shutdown
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    // Command input handling
                    _ if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        if let Some(command) =
                            self.command_input.handle_key(key.code, key.modifiers)
                        {
                            return Ok(Some(command));
                        }
                    }
                    // Delegate all other keys to the focused pane
                    _ if matches!(self.focused_area, FocusedArea::Pane(idx) if idx < self.panes.len()) => {
                        if let FocusedArea::Pane(idx) = self.focused_area {
                            if let Some(pane) = self.panes.get_mut(idx) {
                                use crate::content_pane::{PaneEvent, PaneEventResult};
                                let result =
                                    pane.handle_event(PaneEvent::KeyPress(key.code, key.modifiers));
                                match result {
                                    PaneEventResult::Handled => {}
                                    PaneEventResult::NotHandled => {}
                                    PaneEventResult::RequestClose => {}
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {
                    // Handle terminal resize
                    // TODO: Resize PTY and parser
                }
                _ => {}
            }
        }

        Ok(Some(String::new()))
    }

    /// Runs the main event loop, executing commands in the provided shell.
    ///
    /// This method blocks until the user quits (Ctrl+Q).
    ///
    /// # Errors
    /// Returns an error if rendering or event handling fails
    #[allow(clippy::unused_async)]
    pub async fn run(&mut self) -> Result<()> {
        let source_info = SourceInfo::default();
        let params = ExecutionParameters::default();

        loop {
            // Update the command.
            self.command_input.try_refresh().await;

            // Render the UI
            self.render()?;

            // Handle input events.
            match self.handle_events()? {
                Some(command) if !command.is_empty() => {
                    // User pressed Enter in command pane - execute the command
                    let shell = self.shell.clone();
                    let source_info = source_info.clone();
                    let params = params.clone();
                    tokio::spawn(async move {
                        let mut shell = shell.lock().await;
                        let _ = shell.run_string(command, &source_info, &params).await;
                        drop(shell);
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
}

impl Drop for AppUI {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
