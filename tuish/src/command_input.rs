//! Command input widget for tuish.
//!
//! This module provides a reusable command input component that handles
//! text editing, cursor movement, and rendering.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use tokio::sync::Mutex;

/// A command input widget that maintains its own state and handles text editing.
pub struct CommandInput {
    /// Enablement.
    enabled: bool,
    /// Current input buffer
    buffer: String,
    /// Cursor position in buffer (byte offset)
    cursor_pos: usize,
    /// Whether this widget currently has focus
    focused: bool,
    /// Reference to the shell for prompt rendering
    shell: Arc<Mutex<brush_core::Shell>>,
    /// Cached prompt string (updated during render)
    cached_prompt: String,
}

/// Result of handling a key press in the command input.
pub enum CommandKeyResult {
    /// No action is required.
    NoAction,
    /// Exit is requested.
    RequestExit,
    /// A complete command has been entered.
    CommandEntered(String),
    /// Completion is requested.
    RequestCompletion,
}

impl CommandInput {
    /// Creates a new command input widget.
    ///
    /// # Arguments
    /// * `shell` - Reference to the shell for prompt rendering
    /// * `focused_title` - Title to display when the widget is focused
    /// * `unfocused_title` - Title to display when the widget is not focused
    #[must_use]
    pub fn new(shell: &Arc<Mutex<brush_core::Shell>>) -> Self {
        Self {
            enabled: true,
            buffer: String::new(),
            cursor_pos: 0,
            focused: false,
            shell: shell.clone(),
            cached_prompt: "> ".to_string(),
        }
    }

    /// Updates the cached prompt from the shell.
    #[allow(dead_code)]
    pub async fn try_update_prompt_async(&mut self) {
        if let Ok(mut shell) = self.shell.try_lock() {
            if let Ok(prompt) = shell.compose_prompt().await {
                let mut parser = vt100::Parser::new(1, 1000, 0);
                parser.process(prompt.as_bytes());
                self.cached_prompt = parser.screen().contents();
            }
        }
    }

    /// Sets the cached prompt directly (used by event-driven prompt updates).
    pub fn set_cached_prompt(&mut self, prompt: String) {
        self.cached_prompt = prompt;
    }

    /// Sets the focus state of this widget.
    pub const fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Refreshes the command input state, updating the prompt.
    #[allow(dead_code)]
    pub async fn try_refresh(&mut self) {
        if !self.enabled {
            return;
        }

        self.try_update_prompt_async().await;
    }

    /// Disables the command input (e.g., when a command is running).
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Enables the command input (e.g., when no command is running).
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Returns the current buffer content.
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Returns the current cursor position.
    pub const fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    /// Applies a completion to the buffer.
    ///
    /// This is called from the UI when a completion is accepted.
    pub fn apply_completion(
        &mut self,
        completion: String,
        insertion_index: usize,
        delete_count: usize,
    ) {
        // Remove the text that will be replaced
        let end = insertion_index + delete_count;
        self.buffer.replace_range(insertion_index..end, &completion);

        // Update cursor position
        self.cursor_pos = insertion_index + completion.len();
    }

    /// Handles a key press event and returns the appropriate result.
    #[allow(clippy::string_slice, clippy::map_unwrap_or)]
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> CommandKeyResult {
        if !self.enabled {
            return CommandKeyResult::NoAction;
        }

        match code {
            KeyCode::Tab => {
                // Request completion
                CommandKeyResult::RequestCompletion
            }
            KeyCode::Char('d')
                if modifiers.contains(KeyModifiers::CONTROL) && self.buffer.is_empty() =>
            {
                CommandKeyResult::RequestExit
            }
            KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.insert(self.cursor_pos, c);
                self.cursor_pos += c.len_utf8();
                CommandKeyResult::NoAction
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    let prev_pos = self.buffer[..self.cursor_pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.buffer.remove(prev_pos);
                    self.cursor_pos = prev_pos;
                }
                CommandKeyResult::NoAction
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.buffer.len() {
                    self.buffer.remove(self.cursor_pos);
                }
                CommandKeyResult::NoAction
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    let prev_pos = self.buffer[..self.cursor_pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.cursor_pos = prev_pos;
                }
                CommandKeyResult::NoAction
            }
            KeyCode::Right => {
                if self.cursor_pos < self.buffer.len() {
                    let next_pos = self.buffer[self.cursor_pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor_pos + i)
                        .unwrap_or(self.buffer.len());
                    self.cursor_pos = next_pos;
                }
                CommandKeyResult::NoAction
            }
            KeyCode::Up | KeyCode::Down => {
                // Reserved for future history navigation when completion is not active
                CommandKeyResult::NoAction
            }
            KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor_pos = 0;
                CommandKeyResult::NoAction
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                CommandKeyResult::NoAction
            }
            KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor_pos = self.buffer.len();
                CommandKeyResult::NoAction
            }
            KeyCode::End => {
                self.cursor_pos = self.buffer.len();
                CommandKeyResult::NoAction
            }
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.truncate(self.cursor_pos);
                CommandKeyResult::NoAction
            }
            KeyCode::Enter if !self.buffer.is_empty() => {
                let command = std::mem::take(&mut self.buffer);
                self.cursor_pos = 0;
                CommandKeyResult::CommandEntered(command)
            }
            _ => CommandKeyResult::NoAction,
        }
    }

    /// Calculates the display width of a string containing ANSI escape sequences.
    ///
    /// This strips ANSI codes and returns the actual visible character count.
    fn calculate_display_width(text: &str) -> usize {
        // Use vt100 parser to strip ANSI escape sequences
        let mut parser = vt100::Parser::new(1, 1000, 0);
        parser.process(text.as_bytes());
        parser.screen().contents().chars().count()
    }

    /// Renders the command input widget to the given area.
    ///
    /// # Arguments
    /// * `frame` - The frame to render to
    /// * `area` - The area to render in
    ///
    /// # Returns
    /// The cursor position (x, y) if focused, otherwise `None`
    pub fn render_with_cursor(&self, frame: &mut Frame<'_>, area: Rect) -> Option<(u16, u16)> {
        let bg_color = if !self.enabled {
            Color::Rgb(30, 30, 40)
        } else if self.focused {
            Color::Rgb(30, 25, 40)
        } else {
            Color::Rgb(25, 25, 35)
        };

        let para_style = if self.enabled {
            Style::default().bg(bg_color)
        } else {
            Style::default().fg(Color::DarkGray).bg(bg_color)
        };

        // Build the input line with syntax highlighting
        let input_line = if self.enabled {
            self.build_highlighted_input()
        } else {
            // When disabled, render without highlighting
            let input_text = format!("{}{}", self.cached_prompt, self.buffer);
            Line::from(Span::styled(input_text, para_style))
        };

        // No border - region handles that
        let input_paragraph = Paragraph::new(input_line).style(para_style);
        frame.render_widget(input_paragraph, area);

        // Return cursor position if focused
        if self.focused {
            // Calculate the display width of the prompt (stripping ANSI escape sequences)
            let prompt_display_width = Self::calculate_display_width(&self.cached_prompt);
            // Cursor position: prompt width + cursor_pos (no border offset needed)
            let cursor_x = area.x
                + u16::try_from(prompt_display_width).unwrap_or(0)
                + u16::try_from(self.cursor_pos).unwrap_or(0);
            let cursor_y = area.y;
            Some((cursor_x, cursor_y))
        } else {
            None
        }
    }

    /// Builds a highlighted input line with syntax highlighting.
    fn build_highlighted_input(&self) -> Line<'_> {
        let mut spans = Vec::new();

        // Add the prompt (without highlighting)
        spans.push(Span::raw(self.cached_prompt.clone()));

        // Get syntax highlighting for the buffer
        if let Ok(shell) = self.shell.try_lock() {
            let highlight_spans =
                brush_interactive::highlight_command(&shell, &self.buffer, self.cursor_pos);

            for highlight_span in highlight_spans {
                let text = highlight_span.text(&self.buffer);
                let style = Self::kind_to_style(highlight_span.kind);
                spans.push(Span::styled(text.to_owned(), style));
            }
        } else {
            // If we can't lock the shell, just render without highlighting
            spans.push(Span::raw(self.buffer.clone()));
        }

        Line::from(spans)
    }

    /// Maps a highlight kind to a ratatui style.
    fn kind_to_style(kind: brush_interactive::HighlightKind) -> Style {
        use brush_interactive::HighlightKind;

        match kind {
            HighlightKind::Default => Style::default().fg(Color::Rgb(220, 220, 230)),
            HighlightKind::Comment => Style::default()
                .fg(Color::Rgb(100, 100, 120))
                .add_modifier(Modifier::ITALIC),
            HighlightKind::Arithmetic => Style::default().fg(Color::Rgb(165, 243, 252)), // Cyan
            HighlightKind::Parameter => Style::default().fg(Color::Rgb(244, 114, 182)),  // Pink
            HighlightKind::CommandSubstitution => Style::default().fg(Color::Rgb(165, 243, 252)), /* Cyan */
            HighlightKind::Quoted => Style::default().fg(Color::Rgb(253, 224, 71)), // Yellow
            HighlightKind::Operator => Style::default()
                .fg(Color::Rgb(196, 181, 253)) // Light purple
                .add_modifier(Modifier::ITALIC),
            HighlightKind::Assignment => Style::default().fg(Color::Rgb(150, 150, 170)),
            HighlightKind::HyphenOption => Style::default()
                .fg(Color::Rgb(187, 247, 208)) // Light green
                .add_modifier(Modifier::ITALIC),
            HighlightKind::Function => Style::default()
                .fg(Color::Rgb(253, 224, 71)) // Yellow
                .add_modifier(Modifier::BOLD),
            HighlightKind::Keyword => Style::default()
                .fg(Color::Rgb(251, 146, 60)) // Orange
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            HighlightKind::Builtin => Style::default()
                .fg(Color::Rgb(134, 239, 172)) // Green
                .add_modifier(Modifier::BOLD),
            HighlightKind::Alias => Style::default()
                .fg(Color::Rgb(165, 243, 252)) // Cyan
                .add_modifier(Modifier::BOLD),
            HighlightKind::ExternalCommand => Style::default()
                .fg(Color::Rgb(187, 247, 208)) // Light green
                .add_modifier(Modifier::BOLD),
            HighlightKind::NotFoundCommand => {
                Style::default()
                    .fg(Color::Rgb(248, 113, 113))
                    .add_modifier(Modifier::BOLD) // Red
            }
            HighlightKind::UnknownCommand => Style::default()
                .fg(Color::Rgb(200, 200, 220))
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl crate::content_pane::ContentPane for CommandInput {
    fn name(&self) -> &'static str {
        "Command Input"
    }

    fn kind(&self) -> crate::content_pane::PaneKind {
        crate::content_pane::PaneKind::CommandInput
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
        // CommandInput has its own render_with_cursor implementation
        // This trait implementation ignores the cursor position
        let _ = self.render_with_cursor(frame, area);
    }

    fn handle_event(
        &mut self,
        event: crate::content_pane::PaneEvent,
    ) -> crate::content_pane::PaneEventResult {
        use crate::content_pane::{PaneEvent, PaneEventResult};

        match event {
            PaneEvent::Focused => {
                self.set_focused(true);
                PaneEventResult::Handled
            }
            PaneEvent::Unfocused => {
                self.set_focused(false);
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(code, modifiers) => {
                match self.handle_key(code, modifiers) {
                    CommandKeyResult::NoAction => PaneEventResult::Handled,
                    CommandKeyResult::RequestExit => PaneEventResult::RequestClose, /* Signal exit to AppUI */
                    CommandKeyResult::CommandEntered(cmd) => PaneEventResult::RequestExecute(cmd),
                    CommandKeyResult::RequestCompletion => PaneEventResult::RequestCompletion,
                }
            }
            PaneEvent::Resized { .. } => PaneEventResult::Handled,
        }
    }

    fn border_title(&self) -> Option<String> {
        None // CommandInput renders its own title as part of its border
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}
