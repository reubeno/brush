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
    widgets::{Block, BorderType, Borders, Tabs},
};
use tokio::sync::Mutex;

use std::collections::HashMap;

use crate::command_input::CommandInput;
use crate::completion_pane::CompletionPane;
use crate::content_pane::ContentPane;
use crate::pane_role::PaneRole;
use crate::terminal_pane::TerminalPane;

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
    
    // Special panes with direct access (not in general storage)
    /// Primary terminal pane (needs direct access for output writing)
    primary_terminal: Box<TerminalPane>,
    /// Completion pane (needs direct access for modal workflow)
    completion_pane: Box<CompletionPane>,
    
    /// General content panes (Environment, History, etc.)
    panes: HashMap<PaneRole, Box<dyn ContentPane>>,
    
    /// Order of panes in the tab bar (excluding special panes)
    tab_order: Vec<PaneRole>,
    
    /// Command input widget
    command_input: CommandInput,
    /// Which area currently has focus
    focused_area: FocusedArea,
    /// The pane that was focused before completion was triggered
    pre_completion_focus: Option<FocusedArea>,
}

/// Result of handling a UI event.
pub enum UIEventResult {
    /// The application has been asked to terminate.
    RequestExit,
    /// The application should continue running.
    Continue,
    /// The application should execute the given command.
    ExecuteCommand(String),
    /// The application should trigger completion.
    RequestCompletion,
}

impl AppUI {
    /// Creates a new `AppUI` with the given special panes.
    ///
    /// Use `add_pane` to add general content panes after construction.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to run the UI for
    /// * `primary_terminal` - The primary terminal pane
    /// * `completion_pane` - The completion pane
    pub fn new(
        shell: &Arc<Mutex<brush_core::Shell>>,
        primary_terminal: Box<TerminalPane>,
        completion_pane: Box<CompletionPane>,
    ) -> Self {
        // Initialize the ratatui terminal in raw mode
        let terminal = ratatui::init();

        Self {
            terminal,
            shell: shell.clone(),
            primary_terminal,
            completion_pane,
            panes: HashMap::new(),
            tab_order: Vec::new(),
            command_input: CommandInput::new(shell),
            focused_area: FocusedArea::CommandInput,
            pre_completion_focus: None,
        }
    }

    /// Adds a content pane with the given role.
    ///
    /// The pane will be displayed in tabs in the order added.
    ///
    /// # Panics
    /// Panics if a pane with this role already exists.
    pub fn add_pane(&mut self, role: PaneRole, pane: Box<dyn ContentPane>) {
        if self.panes.contains_key(&role) {
            panic!("Pane with role {:?} already exists", role);
        }
        self.tab_order.push(role);
        self.panes.insert(role, pane);
    }

    /// Returns the total number of visible panes (terminal + general panes).
    fn pane_count(&self) -> usize {
        1 + self.panes.len() // Terminal always visible + general panes
    }

    /// Returns a mutable reference to a pane by tab index.
    ///
    /// Index 0 is always the terminal pane, followed by panes in tab_order.
    fn pane_at_mut(&mut self, index: usize) -> Option<&mut dyn ContentPane> {
        if index == 0 {
            Some(&mut *self.primary_terminal as &mut dyn ContentPane)
        } else {
            let role = self.tab_order.get(index - 1)?;
            self.panes.get_mut(role).map(|p| &mut **p as &mut dyn ContentPane)
        }
    }

    /// Returns an iterator over all pane names for the tab bar.
    fn pane_names(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.primary_terminal.name().to_string())
            .chain(
                self.tab_order
                    .iter()
                    .filter_map(|role| self.panes.get(role).map(|p| p.name().to_string()))
            )
    }

    /// Writes output to the primary terminal pane.
    ///
    /// This allows the UI to display messages in the terminal between command executions.
    pub fn write_to_terminal(&self, data: &[u8]) {
        self.primary_terminal.process_output(data);
    }

    /// Sets the currently running command to display in the terminal pane's border.
    ///
    /// Pass `None` to clear the running command display.
    pub fn set_running_command(&mut self, command: Option<String>) {
        self.primary_terminal.set_running_command(command);
    }

    const CONTENT_PANE_HEIGHT_PERCENTAGE: u16 = 80;

    /// Gets mutable access to a specific pane.
    #[allow(dead_code)]
    pub fn get_pane_mut(&mut self, index: usize) -> Option<&mut dyn ContentPane> {
        self.pane_at_mut(index)
    }

    /// Draws the UI with content panes and command input.
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        let focused_area = self.focused_area;

        // Check if we should show the completion pane instead of normal panes
        let show_completion = self.completion_pane.is_active();

        // Collect tab titles from panes (unless showing completion)
        let tab_titles: Vec<String> = if show_completion {
            vec!["Completions".to_string()]
        } else {
            self.pane_names().map(String::from).collect()
        };

        // Track which pane is selected (for rendering) vs focused (for input)
        let selected_pane_index = match focused_area {
            FocusedArea::Pane(idx) => idx,
            FocusedArea::CommandInput => 0, // Keep showing first pane when command input is focused
        };

        // Update command input focus state
        self.command_input
            .set_focused(matches!(focused_area, FocusedArea::CommandInput));

        // Borrow fields separately to avoid capturing `self` in the closure
        let primary_terminal = &mut self.primary_terminal;
        let completion_pane = &mut self.completion_pane;
        let panes = &mut self.panes;
        let tab_order = &self.tab_order;
        let command_input = &mut self.command_input;

        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(Self::CONTENT_PANE_HEIGHT_PERCENTAGE), /* Tab area (tabs +
                                                                                   * content) */
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

            let colors = [
                Color::Rgb(70, 70, 70),
                Color::Rgb(90, 90, 90),
                Color::Rgb(110, 110, 110),
                Color::Rgb(130, 130, 130),
                Color::Rgb(150, 150, 150),
                Color::Rgb(170, 170, 170),
            ];

            let tabs = Tabs::new(tab_titles.iter().enumerate().map(|(i, t)| {
                Line::from(std::format!(" {t} "))
                    .style(Style::default().bg(colors[i % colors.len()]))
            }))
            .select(tab_selection)
            .style(Style::default().fg(Color::White).bg(Color::DarkGray)) // Unselected tabs
            .highlight_style(Style::default().bg(Color::Green))
            .divider("")
            .padding("", "");
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

            // Get border title from the selected pane if available
            let border_title = if selected_pane_index == 0 {
                primary_terminal.as_ref().border_title()
            } else {
                tab_order
                    .get(selected_pane_index - 1)
                    .and_then(|role| panes.get(role))
                    .and_then(|pane| pane.border_title())
            };

            // Render the selected pane's content with borders
            let mut content_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(content_border_style);

            if let Some(title) = border_title {
                content_block = content_block.title(
                    Line::from(title).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                );
            }

            let content_inner = content_block.inner(tab_area_chunks[1]);
            f.render_widget(content_block, tab_area_chunks[1]);

            // Render the selected pane's content inside the bordered area
            // If completion pane is active, show it instead
            if show_completion {
                completion_pane.as_mut().render(f, content_inner);
            } else if selected_pane_index == 0 {
                primary_terminal.as_mut().render(f, content_inner);
            } else if let Some(role) = tab_order.get(selected_pane_index - 1) {
                if let Some(pane) = panes.get_mut(role) {
                    pane.render(f, content_inner);
                }
            }

            // Render the command input pane using the widget
            if let Some((cursor_x, cursor_y)) = command_input.render(f, chunks[1]) {
                f.set_cursor_position((cursor_x, cursor_y));
            }
        })?;

        Ok(())
    }

    fn set_focus_to_command_input(&mut self) {
        // Send Unfocused event to currently focused pane
        if let FocusedArea::Pane(idx) = self.focused_area {
            if let Some(pane) = self.pane_at_mut(idx) {
                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
            }
        }
        self.focused_area = FocusedArea::CommandInput;
    }

    fn set_focus_to_next_pane_or_area(&mut self) {
        let num_panes = self.pane_count();
        let old_focused_area = self.focused_area;

        self.focused_area = match self.focused_area {
            FocusedArea::Pane(idx) if idx + 1 < num_panes => FocusedArea::Pane(idx + 1),
            FocusedArea::Pane(_) => {
                if self.command_input.is_enabled() {
                    FocusedArea::CommandInput
                } else {
                    FocusedArea::Pane(0)
                }
            }
            FocusedArea::CommandInput => FocusedArea::Pane(0),
        };

        // Send Unfocused event to previously focused pane
        if let FocusedArea::Pane(old_idx) = old_focused_area {
            if let Some(pane) = self.pane_at_mut(old_idx) {
                let _ = pane.handle_event(crate::content_pane::PaneEvent::Unfocused);
            }
        }

        // Send Focused event to newly focused pane
        if let FocusedArea::Pane(new_idx) = self.focused_area {
            if let Some(pane) = self.pane_at_mut(new_idx) {
                let _ = pane.handle_event(crate::content_pane::PaneEvent::Focused);
            }
        }
    }

    /// Handles input events.
    #[allow(clippy::too_many_lines)]
    pub fn handle_events(&mut self) -> Result<UIEventResult, std::io::Error> {
        // Check if completion pane is active
        let completion_active = self.completion_pane.is_active();

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    // If completion is active, handle special keys
                    KeyCode::Esc if completion_active => {
                        // Cancel completion
                        self.completion_pane.clear();
                        // Restore focus
                        if let Some(prev_focus) = self.pre_completion_focus.take() {
                            self.focused_area = prev_focus;
                        }
                    }
                    KeyCode::Enter if completion_active => {
                        // Accept completion
                        if let Some(completion) = self.completion_pane.selected_completion() {
                            let (insertion_index, delete_count) = self.completion_pane.insertion_params();
                            // Apply to command input
                            self.command_input.apply_completion(
                                completion,
                                insertion_index,
                                delete_count,
                            );
                        }
                        self.completion_pane.clear();
                        // Restore focus
                        if let Some(prev_focus) = self.pre_completion_focus.take() {
                            self.focused_area = prev_focus;
                        }
                    }
                    // Tab/Shift-Tab for navigation when completion is active
                    KeyCode::Tab
                        if completion_active && !key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        self.completion_pane.handle_event(crate::content_pane::PaneEvent::KeyPress(
                            KeyCode::Down,
                            KeyModifiers::empty(),
                        ));
                    }
                    KeyCode::BackTab | KeyCode::Tab
                        if completion_active && key.modifiers.contains(KeyModifiers::SHIFT) =>
                    {
                        self.completion_pane.handle_event(crate::content_pane::PaneEvent::KeyPress(
                            KeyCode::Up,
                            KeyModifiers::empty(),
                        ));
                    }
                    // Arrow keys for navigation
                    KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End
                        if completion_active =>
                    {
                        self.completion_pane.handle_event(crate::content_pane::PaneEvent::KeyPress(
                            key.code,
                            key.modifiers,
                        ));
                    }
                    // Allow typing to update buffer and re-trigger completion
                    _ if completion_active => {
                        // First, let command input handle the key (updates buffer)
                        match self.command_input.handle_key(key.code, key.modifiers) {
                            crate::command_input::CommandKeyResult::NoAction => {
                                // Buffer was updated, re-trigger completion
                                return Ok(UIEventResult::RequestCompletion);
                            }
                            crate::command_input::CommandKeyResult::CommandEntered(command) => {
                                // User pressed Enter with actual command text - cancel completion and execute
                                self.completion_pane.clear();
                                if let Some(prev_focus) = self.pre_completion_focus.take() {
                                    self.focused_area = prev_focus;
                                }
                                return Ok(UIEventResult::ExecuteCommand(command));
                            }
                            _ => {
                                // Cancel completion for other cases (e.g., Ctrl+D)
                                self.completion_pane.clear();
                                if let Some(prev_focus) = self.pre_completion_focus.take() {
                                    self.focused_area = prev_focus;
                                }
                            }
                        }
                    }
                    // Ctrl+Space cycles focus through panes and command input
                    KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.set_focus_to_next_pane_or_area();
                    }
                    // Ctrl+Q quits the application by returning None to signal shutdown
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(UIEventResult::RequestExit);
                    }
                    // Command input handling
                    _ if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        match self.command_input.handle_key(key.code, key.modifiers) {
                            crate::command_input::CommandKeyResult::RequestExit => {
                                return Ok(UIEventResult::RequestExit);
                            }
                            crate::command_input::CommandKeyResult::NoAction => {}
                            crate::command_input::CommandKeyResult::CommandEntered(command) => {
                                return Ok(UIEventResult::ExecuteCommand(command));
                            }
                            crate::command_input::CommandKeyResult::RequestCompletion => {
                                return Ok(UIEventResult::RequestCompletion);
                            }
                        }
                    }
                    // Delegate all other keys to the focused pane
                    _ if matches!(self.focused_area, FocusedArea::Pane(idx) if idx < self.pane_count()) => {
                        if let FocusedArea::Pane(idx) = self.focused_area {
                            if let Some(pane) = self.pane_at_mut(idx) {
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

        Ok(UIEventResult::Continue)
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

        let mut running_command: Option<
            tokio::task::JoinHandle<Result<brush_core::ExecutionResult, brush_core::Error>>,
        > = None;

        loop {
            // See if we're waiting on a command (and it finished).
            if let Some(handle) = &mut running_command {
                if handle.is_finished() {
                    if let Ok(result) = handle.await? {
                        // Write a status message to the terminal pane
                        let exit_code: u8 = (&result.exit_code).into();
                        let status_msg = std::format!(
                            "\r\n----------- [tuish: command exited with code {exit_code}] ----------- \r\n\r\n"
                        );
                        self.write_to_terminal(status_msg.as_bytes());

                        if matches!(
                            result.next_control_flow,
                            brush_core::ExecutionControlFlow::ExitShell
                        ) {
                            break;
                        }
                    }

                    running_command = None;

                    // Clear the running command display
                    self.set_running_command(None);

                    self.command_input.enable();
                    self.set_focus_to_command_input();
                }
            }

            // Update the command input area if it's not busy.
            self.command_input.try_refresh().await;

            // Render the UI
            self.render()?;

            // Handle input events.
            match self.handle_events()? {
                UIEventResult::ExecuteCommand(command) => {
                    // User pressed Enter in command pane - execute the command
                    let shell = self.shell.clone();
                    let source_info = source_info.clone();
                    let params = params.clone();

                    // Show the running command in the terminal pane's border
                    self.set_running_command(Some(command.clone()));

                    running_command = Some(tokio::spawn(async move {
                        let mut shell = shell.lock().await;
                        let result = shell.run_string(command, &source_info, &params).await;
                        drop(shell);

                        result
                    }));

                    // Once it's running, disable the command area.
                    self.command_input.disable();
                    self.set_focus_to_next_pane_or_area();

                    // TODO: Check for exit signal from command execution
                }
                UIEventResult::RequestCompletion => {
                    // User pressed Tab - trigger completion
                    let mut shell = self.shell.lock().await;
                    let buffer = self.command_input.buffer();
                    let cursor_pos = self.command_input.cursor_pos();

                    if let Ok(completions) = shell.complete(buffer, cursor_pos).await {
                        drop(shell); // Release lock

                        if completions.candidates.is_empty() {
                            // No completions available
                        } else if completions.candidates.len() == 1 {
                            // Auto-accept single completion
                            let completion = completions.candidates.into_iter().next().unwrap();
                            self.command_input.apply_completion(
                                completion,
                                completions.insertion_index,
                                completions.delete_count,
                            );
                        } else {
                            // Multiple completions - show pane
                            // Store current focus to restore later
                            self.pre_completion_focus = Some(self.focused_area);

                            // Show completion pane
                            self.completion_pane.set_completions(completions);
                        }
                    } else {
                        drop(shell);
                        tracing::debug!("Completion failed");
                    }
                }
                UIEventResult::Continue => {}
                UIEventResult::RequestExit => {
                    // User requested exit (Ctrl+Q)
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
