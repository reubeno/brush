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
    widgets::{Block, BorderType, Borders, Paragraph},
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
    /// Title to display when focused
    focused_title: &'static str,
    /// Title to display when not focused
    unfocused_title: &'static str,
    /// Title to display when disabled
    disabled_title: &'static str,
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
            focused_title: "Command Input [FOCUSED - Ctrl+Space to switch, Ctrl+Q to quit]",
            unfocused_title: "Command Input [Ctrl+Space to focus, Ctrl+Q to quit]",
            disabled_title: "Command is running...",
            shell: shell.clone(),
            cached_prompt: "> ".to_string(),
        }
    }

    /// Updates the cached prompt from the shell.
    pub async fn try_update_prompt_async(&mut self) {
        if let Ok(mut shell) = self.shell.try_lock() {
            if let Ok(prompt) = shell.compose_prompt().await {
                let mut parser = vt100::Parser::new(1, 1000, 0);
                parser.process(prompt.as_bytes());
                self.cached_prompt = parser.screen().contents();
            }
        }
    }

    /// Sets the focus state of this widget.
    pub const fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Refreshes the command input state, updating the prompt.
    pub async fn try_refresh(&mut self) {
        if !self.enabled {
            return;
        }

        self.try_update_prompt_async().await;
    }

    /// Disables the command input (e.g., when a command is running).
    pub const fn disable(&mut self) {
        self.enabled = false;
    }

    /// Enables the command input (e.g., when no command is running).
    pub const fn enable(&mut self) {
        self.enabled = true;
    }

    /// Checks if the command input is enabled.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
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
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) -> Option<(u16, u16)> {
        let (title, border_style) = if !self.enabled {
            (self.disabled_title, Style::default().fg(Color::DarkGray))
        } else if self.focused {
            (
                self.focused_title,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (self.unfocused_title, Style::default().fg(Color::DarkGray))
        };

        let input_text = format!("{}{}", self.cached_prompt, self.buffer);

        let para_style = if self.enabled {
            Style::default()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let input_paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_type(BorderType::Rounded)
                    .title(title)
                    .border_style(border_style),
            )
            .style(para_style);
        frame.render_widget(input_paragraph, area);

        // Return cursor position if focused
        if self.focused {
            // Calculate the display width of the prompt (stripping ANSI escape sequences)
            let prompt_display_width = Self::calculate_display_width(&self.cached_prompt);
            // Cursor position: prompt width + cursor_pos + left border (1)
            let cursor_x = area.x
                + 1
                + u16::try_from(prompt_display_width).unwrap_or(0)
                + u16::try_from(self.cursor_pos).unwrap_or(0);
            let cursor_y = area.y + 1;
            Some((cursor_x, cursor_y))
        } else {
            None
        }
    }
}
