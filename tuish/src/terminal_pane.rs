//! Terminal content pane using `tui_term` for PTY display.

#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use bytes::Bytes;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use tokio::sync::mpsc::Sender;
use tui_term::widget::PseudoTerminal;

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

/// Maximum length for command display in the border title.
const MAX_COMMAND_DISPLAY_LENGTH: usize = 40;

/// A content pane that displays a `PTY` terminal using `tui_term`.
pub struct TerminalPane {
    parser: Arc<RwLock<vt100::Parser>>,
    pty_writer: Sender<Bytes>,
    pty_handle: std::sync::Arc<crate::pty::Pty>,
    /// The currently running command, if any.
    running_command: Option<String>,
    /// Whether the terminal pane is currently focused.
    is_focused: bool,
    /// Last known dimensions (rows, cols) of the rendered area
    last_dimensions: (u16, u16),
}

impl TerminalPane {
    /// Create a new terminal pane with the given PTY resources.
    pub fn new(
        parser: Arc<RwLock<vt100::Parser>>,
        pty_writer: Sender<Bytes>,
        pty_handle: std::sync::Arc<crate::pty::Pty>,
    ) -> Self {
        Self {
            parser,
            pty_writer,
            pty_handle,
            running_command: None,
            is_focused: false,
            last_dimensions: (0, 0),
        }
    }

    /// Sets the currently running command to display in the border.
    pub fn set_running_command(&mut self, command: Option<String>) {
        self.running_command = command;
    }

    /// Sends raw bytes to the PTY.
    fn send_to_pty(&self, bytes: impl AsRef<[u8]>) {
        let _ = self
            .pty_writer
            .try_send(Bytes::copy_from_slice(bytes.as_ref()));
    }

    /// Process output from the PTY and forward to the parser
    pub fn process_output(&self, data: &[u8]) {
        let mut parser = self.parser.write().unwrap();
        parser.process(data);
    }
}

impl ContentPane for TerminalPane {
    fn name(&self) -> &'static str {
        "Terminal"
    }

    fn kind(&self) -> PaneKind {
        PaneKind::Terminal
    }

    fn border_title(&self) -> Option<String> {
        self.running_command.as_ref().map(|cmd| {
            let trimmed = cmd.trim();
            if trimmed.chars().count() > MAX_COMMAND_DISPLAY_LENGTH {
                let truncated: String = trimmed.chars().take(MAX_COMMAND_DISPLAY_LENGTH).collect();
                format!("Running: {truncated}...")
            } else {
                format!("Running: {trimmed}")
            }
        })
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Add subtle background for depth
        let bg =
            ratatui::widgets::Block::default().style(Style::default().bg(Color::Rgb(18, 18, 26)));
        frame.render_widget(bg, area);

        // Resize parser and PTY if area dimensions changed
        let new_dimensions = (area.height, area.width);
        if new_dimensions != self.last_dimensions && area.height > 0 && area.width > 0 {
            // Resize both parser and PTY to match the actual rendered area
            if let Err(e) = self.pty_handle.resize(area.height, area.width) {
                tracing::warn!("Failed to resize PTY: {}", e);
            }
            self.last_dimensions = new_dimensions;
        }

        let screen = {
            let parser = self.parser.read().unwrap();
            parser.screen().clone()
        };

        // Hide the cursor when the terminal pane is not focused
        let cursor = tui_term::widget::Cursor::default().visibility(self.is_focused);
        let pseudo_term = PseudoTerminal::new(&screen).cursor(cursor);
        frame.render_widget(pseudo_term, area);
    }

    fn handle_event(&mut self, event: PaneEvent) -> PaneEventResult {
        match event {
            PaneEvent::Focused => {
                self.is_focused = true;
                PaneEventResult::Handled
            }
            PaneEvent::Unfocused => {
                self.is_focused = false;
                PaneEventResult::Handled
            }
            PaneEvent::KeyPress(key, modifiers) => {
                // Forward all keyboard input to PTY when we're focused
                match key {
                    KeyCode::Char(c) if modifiers.contains(KeyModifiers::CONTROL) => {
                        // Handle Ctrl+key combinations
                        if let 'a'..='z' = c {
                            // Ctrl+A = 1, Ctrl+B = 2, ..., Ctrl+Z = 26
                            let ctrl_code = c as u8 - b'a' + 1;
                            self.send_to_pty(vec![ctrl_code]);
                        }
                    }
                    KeyCode::Char(c) => {
                        self.send_to_pty(c.to_string().into_bytes());
                    }
                    KeyCode::Enter => {
                        self.send_to_pty(vec![b'\r']);
                    }
                    KeyCode::Backspace => {
                        self.send_to_pty(vec![0x7f]);
                    }
                    KeyCode::Tab => {
                        self.send_to_pty(vec![b'\t']);
                    }
                    KeyCode::Esc => {
                        self.send_to_pty(vec![0x1b]);
                    }
                    KeyCode::Up => {
                        self.send_to_pty(vec![0x1b, b'[', b'A']);
                    }
                    KeyCode::Down => {
                        self.send_to_pty(vec![0x1b, b'[', b'B']);
                    }
                    KeyCode::Right => {
                        self.send_to_pty(vec![0x1b, b'[', b'C']);
                    }
                    KeyCode::Left => {
                        self.send_to_pty(vec![0x1b, b'[', b'D']);
                    }
                    KeyCode::Home => {
                        self.send_to_pty(vec![0x1b, b'[', b'H']);
                    }
                    KeyCode::End => {
                        self.send_to_pty(vec![0x1b, b'[', b'F']);
                    }
                    KeyCode::PageUp => {
                        self.send_to_pty(vec![0x1b, b'[', b'5', b'~']);
                    }
                    KeyCode::PageDown => {
                        self.send_to_pty(vec![0x1b, b'[', b'6', b'~']);
                    }
                    KeyCode::Delete => {
                        self.send_to_pty(vec![0x1b, b'[', b'3', b'~']);
                    }
                    KeyCode::Insert => {
                        self.send_to_pty(vec![0x1b, b'[', b'2', b'~']);
                    }
                    KeyCode::F(n) => {
                        let seq = match n {
                            1 => vec![0x1b, b'O', b'P'],
                            2 => vec![0x1b, b'O', b'Q'],
                            3 => vec![0x1b, b'O', b'R'],
                            4 => vec![0x1b, b'O', b'S'],
                            5 => vec![0x1b, b'[', b'1', b'5', b'~'],
                            6 => vec![0x1b, b'[', b'1', b'7', b'~'],
                            7 => vec![0x1b, b'[', b'1', b'8', b'~'],
                            8 => vec![0x1b, b'[', b'1', b'9', b'~'],
                            9 => vec![0x1b, b'[', b'2', b'0', b'~'],
                            10 => vec![0x1b, b'[', b'2', b'1', b'~'],
                            11 => vec![0x1b, b'[', b'2', b'3', b'~'],
                            12 => vec![0x1b, b'[', b'2', b'4', b'~'],
                            _ => return PaneEventResult::NotHandled,
                        };
                        self.send_to_pty(seq);
                    }
                    _ => return PaneEventResult::NotHandled,
                }
                PaneEventResult::Handled
            }
            PaneEvent::Resized { .. } => PaneEventResult::NotHandled,
        }
    }
}
