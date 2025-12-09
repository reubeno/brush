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
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::Mutex;

/// A command input widget that maintains its own state and handles text editing.
pub struct CommandInput {
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
    /// Reference to the shell for prompt rendering
    shell: Arc<Mutex<brush_core::Shell>>,
    /// Cached prompt string (updated during render)
    cached_prompt: String,
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
            buffer: String::new(),
            cursor_pos: 0,
            focused: false,
            focused_title: "Command Input [FOCUSED - Ctrl+Space to switch, Ctrl+Q to quit]",
            unfocused_title: "Command Input [Ctrl+Space to focus, Ctrl+Q to quit]",
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
        self.try_update_prompt_async().await;
    }

    /// Handles a key press event and returns `Some(command)` if Enter was pressed.
    ///
    /// # Returns
    /// - `Some(String)` - The complete command when Enter is pressed
    /// - `None` - For all other keys
    #[allow(clippy::string_slice, clippy::map_unwrap_or)]
    pub fn handle_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) -> Option<String> {
        match code {
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor_pos, c);
                self.cursor_pos += c.len_utf8();
                None
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
                None
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.buffer.len() {
                    self.buffer.remove(self.cursor_pos);
                }
                None
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
                None
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
                None
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                None
            }
            KeyCode::End => {
                self.cursor_pos = self.buffer.len();
                None
            }
            KeyCode::Enter => {
                let command = self.buffer.clone();
                self.buffer.clear();
                self.cursor_pos = 0;
                Some(command)
            }
            _ => None,
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
        let (title, border_style) = if self.focused {
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
        let input_paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .style(Style::default());
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
