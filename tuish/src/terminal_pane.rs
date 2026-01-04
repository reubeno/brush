//! Terminal content pane with command-grouped history.
//!
//! This module provides a terminal pane that displays command output in discrete,
//! navigable blocks. Completed commands appear as styled text blocks, stacked
//! vertically with the live terminal at the bottom.

use std::sync::{Arc, RwLock};

use ansi_to_tui::IntoText;
use bytes::Bytes;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use tokio::sync::mpsc::Sender;
use tui_term::widget::PseudoTerminal;

use crate::content_pane::{ContentPane, PaneEvent, PaneEventResult, PaneKind};

/// Maximum length for command display in block headers.
const MAX_COMMAND_DISPLAY_LENGTH: usize = 50;

/// Maximum number of command output blocks to keep in history.
const MAX_HISTORY_BLOCKS: usize = 100;

/// Minimum height allocated to the live terminal area.
const MIN_LIVE_TERMINAL_HEIGHT: u16 = 4;

/// A completed command's output, stored as styled text.
#[derive(Clone)]
pub struct CommandOutputBlock {
    /// The command that was executed.
    pub command: String,
    /// Exit code of the command (None if interrupted).
    pub exit_code: Option<u8>,
    /// Styled output lines from the command.
    pub lines: Vec<Line<'static>>,
    /// Timestamp when the command completed.
    pub timestamp: chrono::DateTime<chrono::Local>,
}

impl CommandOutputBlock {
    /// Creates a new command output block.
    fn new(command: String) -> Self {
        Self {
            command,
            exit_code: None,
            lines: Vec::new(),
            timestamp: chrono::Local::now(),
        }
    }

    /// Returns the number of content lines (excluding empty trailing lines).
    fn content_line_count(&self) -> usize {
        // Trim trailing empty lines
        let mut count = self.lines.len();
        for line in self.lines.iter().rev() {
            if line.spans.is_empty() || line.spans.iter().all(|s| s.content.trim().is_empty()) {
                count = count.saturating_sub(1);
            } else {
                break;
            }
        }
        count.max(1) // At least 1 line
    }
}

/// A content pane that displays PTY terminal output with command-grouped history.
///
/// Shows stacked command output blocks with the live terminal at the bottom.
/// Supports scrolling through history while maintaining the live terminal visible.
pub struct TerminalPane {
    /// History of completed command output blocks.
    history: Vec<CommandOutputBlock>,
    /// The shared vt100 parser from PTY.
    shared_parser: Arc<RwLock<vt100::Parser>>,
    /// Channel for writing to the PTY.
    pty_writer: Sender<Bytes>,
    /// Handle to the PTY for resize operations.
    pty_handle: Arc<crate::pty::Pty>,
    /// The currently running command text, if any.
    running_command: Option<String>,
    /// Whether the terminal pane is currently focused.
    is_focused: bool,
    /// Last known dimensions (rows, cols) for PTY resize.
    last_dimensions: (u16, u16),
    /// Scroll offset (in lines) for viewing history.
    scroll_offset: usize,
    /// Total scrollable content height (cached).
    total_content_height: usize,
}

impl TerminalPane {
    /// Creates a new terminal pane with the given PTY resources.
    pub fn new(
        parser: Arc<RwLock<vt100::Parser>>,
        pty_writer: Sender<Bytes>,
        pty_handle: Arc<crate::pty::Pty>,
    ) -> Self {
        Self {
            history: Vec::new(),
            shared_parser: parser,
            pty_writer,
            pty_handle,
            running_command: None,
            is_focused: false,
            last_dimensions: (0, 0),
            scroll_offset: 0,
            total_content_height: 0,
        }
    }

    /// Sets the currently running command.
    pub fn set_running_command(&mut self, command: Option<String>) {
        self.running_command = command;
    }

    /// Finalizes the current command's output and stores it in history.
    pub fn finalize_command(&mut self, exit_code: Option<u8>) {
        let Some(ref command) = self.running_command else {
            return;
        };

        // Don't capture alternate screen content
        let in_alternate_screen = {
            let parser = self.shared_parser.read().unwrap();
            parser.screen().alternate_screen()
        };

        if !in_alternate_screen {
            let lines = Self::capture_styled_output(&self.shared_parser);

            let mut block = CommandOutputBlock::new(command.clone());
            block.exit_code = exit_code;
            block.lines = lines;

            self.history.push(block);
            if self.history.len() > MAX_HISTORY_BLOCKS {
                self.history.remove(0);
            }

            // Clear the terminal for the next command
            let mut parser = self.shared_parser.write().unwrap();
            parser.process(b"\x1b[2J\x1b[H");
        }

        self.running_command = None;
        self.recalculate_content_height();
        // Auto-scroll to bottom to show new content
        self.scroll_offset = usize::MAX;
    }

    /// Captures styled output from the vt100 parser.
    #[allow(clippy::significant_drop_tightening)]
    fn capture_styled_output(parser: &Arc<RwLock<vt100::Parser>>) -> Vec<Line<'static>> {
        let parser_guard = parser.read().unwrap();
        let screen = parser_guard.screen();
        let formatted = screen.contents_formatted();
        drop(parser_guard);

        if let Ok(text) = formatted.into_text() {
            text.lines
                .into_iter()
                .map(|line| {
                    let owned_spans: Vec<Span<'static>> = line
                        .spans
                        .into_iter()
                        .map(|span| Span::styled(span.content.into_owned(), span.style))
                        .collect();
                    Line::from(owned_spans)
                })
                .collect()
        } else {
            let parser_guard = parser.read().unwrap();
            let contents = parser_guard.screen().contents();
            contents.lines().map(|s| Line::raw(s.to_string())).collect()
        }
    }

    /// Recalculates the total scrollable content height.
    fn recalculate_content_height(&mut self) {
        self.total_content_height = self
            .history
            .iter()
            .map(|b| b.content_line_count() + 2) // +2 for header and separator
            .sum();
    }

    /// Sends raw bytes to the PTY.
    fn send_to_pty(&self, bytes: impl AsRef<[u8]>) {
        let _ = self
            .pty_writer
            .try_send(Bytes::copy_from_slice(bytes.as_ref()));
    }

    /// Returns whether we're in alternate screen mode.
    fn is_alternate_screen(&self) -> bool {
        let parser = self.shared_parser.read().unwrap();
        parser.screen().alternate_screen()
    }

    /// Renders a command block header line.
    fn render_block_header(block: &CommandOutputBlock, width: u16) -> Line<'static> {
        let exit_indicator = match block.exit_code {
            Some(0) => ("✓", Color::Green),
            Some(_code) => ("✗", Color::Red),
            None => ("⚠", Color::Yellow),
        };

        let cmd_display: String = if block.command.chars().count() > MAX_COMMAND_DISPLAY_LENGTH {
            let truncated: String = block
                .command
                .chars()
                .take(MAX_COMMAND_DISPLAY_LENGTH)
                .collect();
            format!("{truncated}...")
        } else {
            block.command.clone()
        };

        let timestamp = block.timestamp.format("%H:%M:%S").to_string();

        Line::from(vec![
            Span::styled(
                format!("─── {}", exit_indicator.0),
                Style::default().fg(exit_indicator.1),
            ),
            Span::styled(
                format!(" {cmd_display} "),
                Style::default()
                    .fg(Color::Rgb(180, 180, 200))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{timestamp}] "),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "─".repeat(
                    (width as usize).saturating_sub(cmd_display.len() + timestamp.len() + 15),
                ),
                Style::default().fg(Color::Rgb(60, 60, 80)),
            ),
        ])
    }

    /// Renders all history blocks and the live terminal.
    #[allow(clippy::too_many_lines)]
    fn render_stacked_view(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if area.height < 2 {
            return;
        }

        // Reserve space for live terminal at the bottom
        let live_height = if self.running_command.is_some() {
            // When a command is running, give it more space
            (area.height / 2).max(MIN_LIVE_TERMINAL_HEIGHT)
        } else {
            // When idle, just show the prompt
            MIN_LIVE_TERMINAL_HEIGHT
        };

        let history_height = area.height.saturating_sub(live_height);

        // Render history blocks in the top area
        if history_height > 0 && !self.history.is_empty() {
            let history_area = Rect::new(area.x, area.y, area.width, history_height);
            self.render_history_blocks(frame, history_area);
        }

        // Render live terminal at the bottom
        let live_area = Rect::new(area.x, area.y + history_height, area.width, live_height);
        self.render_live_terminal(frame, live_area);
    }

    /// Renders the history blocks section.
    fn render_history_blocks(&self, frame: &mut Frame<'_>, area: Rect) {
        // Build all lines from history blocks
        let mut all_lines: Vec<Line<'static>> = Vec::new();

        for block in &self.history {
            // Add header
            all_lines.push(Self::render_block_header(block, area.width));

            // Add content lines (trimmed)
            let content_count = block.content_line_count();
            for line in block.lines.iter().take(content_count) {
                all_lines.push(line.clone());
            }

            // Add subtle separator
            all_lines.push(Line::raw(""));
        }

        // Apply scroll offset
        let visible_height = area.height as usize;
        let total_lines = all_lines.len();

        // Auto-scroll to bottom if at the end
        let max_scroll = total_lines.saturating_sub(visible_height);
        let effective_scroll = self.scroll_offset.min(max_scroll);

        let visible_lines: Vec<Line<'static>> = all_lines
            .into_iter()
            .skip(effective_scroll)
            .take(visible_height)
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        frame.render_widget(paragraph, area);

        // Render scrollbar if needed
        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(total_lines)
                .position(effective_scroll);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Rgb(100, 100, 130)));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    /// Renders the live terminal section.
    #[allow(clippy::significant_drop_tightening)]
    fn render_live_terminal(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Draw a subtle separator line at the top
        if area.height > 1 {
            let separator = Line::from(vec![
                Span::styled(
                    "─".repeat(area.width as usize / 3),
                    Style::default().fg(Color::Rgb(80, 80, 100)),
                ),
                Span::styled(" live ", Style::default().fg(Color::Rgb(139, 92, 246))),
                Span::styled(
                    "─".repeat(area.width as usize / 3),
                    Style::default().fg(Color::Rgb(80, 80, 100)),
                ),
            ]);
            let sep_area = Rect::new(area.x, area.y, area.width, 1);
            frame.render_widget(
                Paragraph::new(separator).alignment(Alignment::Center),
                sep_area,
            );
        }

        // Terminal area below separator
        let term_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );

        // Resize PTY if needed
        let new_dimensions = (term_area.height, term_area.width);
        if new_dimensions != self.last_dimensions && term_area.height > 0 && term_area.width > 0 {
            if let Err(e) = self.pty_handle.resize(term_area.height, term_area.width) {
                tracing::warn!("Failed to resize PTY: {}", e);
            }
            self.last_dimensions = new_dimensions;
        }

        let parser = self.shared_parser.read().unwrap();
        let screen = parser.screen();
        let cursor = tui_term::widget::Cursor::default().visibility(self.is_focused);
        let pseudo_term = PseudoTerminal::new(screen).cursor(cursor);
        frame.render_widget(pseudo_term, term_area);
    }

    /// Scrolls history up (negative) or down (positive).
    fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scrolls history down.
    fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    /// Returns the line offset where a given block starts.
    fn block_start_offset(&self, block_idx: usize) -> usize {
        let mut offset = 0;
        for (idx, block) in self.history.iter().enumerate() {
            if idx == block_idx {
                return offset;
            }
            // Each block has: 1 header line + content lines + 1 empty separator
            offset += 1 + block.content_line_count() + 1;
        }
        offset
    }

    /// Jumps to the previous block.
    fn jump_to_previous_block(&mut self) {
        if self.history.is_empty() {
            return;
        }
        // Find which block we're currently in
        let mut current_block = 0;
        let mut offset = 0;
        for (idx, block) in self.history.iter().enumerate() {
            let block_end = offset + 1 + block.content_line_count() + 1;
            if self.scroll_offset < block_end {
                current_block = idx;
                break;
            }
            offset = block_end;
            current_block = idx;
        }
        // Jump to previous block (or start of current if not at start)
        if self.scroll_offset > self.block_start_offset(current_block) {
            self.scroll_offset = self.block_start_offset(current_block);
        } else if current_block > 0 {
            self.scroll_offset = self.block_start_offset(current_block - 1);
        } else {
            self.scroll_offset = 0;
        }
    }

    /// Jumps to the next block.
    fn jump_to_next_block(&mut self) {
        if self.history.is_empty() {
            return;
        }
        // Find which block we're currently in
        let mut current_block = 0;
        let mut offset = 0;
        for (idx, block) in self.history.iter().enumerate() {
            let block_end = offset + 1 + block.content_line_count() + 1;
            if self.scroll_offset < block_end {
                current_block = idx;
                break;
            }
            offset = block_end;
            current_block = idx;
        }
        // Jump to next block
        if current_block + 1 < self.history.len() {
            self.scroll_offset = self.block_start_offset(current_block + 1);
        } else {
            // Already at last block, scroll to end
            self.scroll_offset = usize::MAX;
        }
    }

    /// Renders the terminal fullscreen (for alternate screen mode like vim/nvim).
    #[allow(clippy::significant_drop_tightening)]
    fn render_fullscreen_terminal(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Resize PTY to full area
        let new_dimensions = (area.height, area.width);
        if new_dimensions != self.last_dimensions && area.height > 0 && area.width > 0 {
            if let Err(e) = self.pty_handle.resize(area.height, area.width) {
                tracing::warn!("Failed to resize PTY: {}", e);
            }
            self.last_dimensions = new_dimensions;
        }

        let parser = self.shared_parser.read().unwrap();
        let screen = parser.screen();
        let cursor = tui_term::widget::Cursor::default().visibility(self.is_focused);
        let pseudo_term = PseudoTerminal::new(screen).cursor(cursor);
        frame.render_widget(pseudo_term, area);
    }

    /// Handles a key press to be forwarded to the PTY.
    fn handle_pty_key(&self, key: KeyCode, modifiers: KeyModifiers) -> PaneEventResult {
        match key {
            KeyCode::Char(c) if modifiers.contains(KeyModifiers::CONTROL) => {
                if let 'a'..='z' = c {
                    let ctrl_code = c as u8 - b'a' + 1;
                    self.send_to_pty(vec![ctrl_code]);
                }
            }
            KeyCode::Char(c) => {
                self.send_to_pty(c.to_string().into_bytes());
            }
            KeyCode::Enter => self.send_to_pty(vec![b'\r']),
            KeyCode::Backspace => self.send_to_pty(vec![0x7f]),
            KeyCode::Tab => self.send_to_pty(vec![b'\t']),
            KeyCode::Esc => self.send_to_pty(vec![0x1b]),
            KeyCode::Up => self.send_to_pty(vec![0x1b, b'[', b'A']),
            KeyCode::Down => self.send_to_pty(vec![0x1b, b'[', b'B']),
            KeyCode::Right => self.send_to_pty(vec![0x1b, b'[', b'C']),
            KeyCode::Left => self.send_to_pty(vec![0x1b, b'[', b'D']),
            KeyCode::Home => self.send_to_pty(vec![0x1b, b'[', b'H']),
            KeyCode::End => self.send_to_pty(vec![0x1b, b'[', b'F']),
            KeyCode::PageUp => self.send_to_pty(vec![0x1b, b'[', b'5', b'~']),
            KeyCode::PageDown => self.send_to_pty(vec![0x1b, b'[', b'6', b'~']),
            KeyCode::Delete => self.send_to_pty(vec![0x1b, b'[', b'3', b'~']),
            KeyCode::Insert => self.send_to_pty(vec![0x1b, b'[', b'2', b'~']),
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

    /// Processes output from the PTY (for compatibility).
    #[allow(dead_code)]
    pub fn process_output(&self, data: &[u8]) {
        let mut parser = self.shared_parser.write().unwrap();
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
        // Subtle background
        let bg =
            ratatui::widgets::Block::default().style(Style::default().bg(Color::Rgb(18, 18, 26)));
        frame.render_widget(bg, area);

        // In alternate screen mode, show only the live terminal (full screen, no separator)
        if self.is_alternate_screen() {
            self.render_fullscreen_terminal(frame, area);
            return;
        }

        // Otherwise show stacked history + live terminal
        self.render_stacked_view(frame, area);
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
                // Ctrl+Shift+Up/Down jumps between blocks
                if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) {
                    match key {
                        KeyCode::Up => {
                            self.jump_to_previous_block();
                            return PaneEventResult::Handled;
                        }
                        KeyCode::Down => {
                            self.jump_to_next_block();
                            return PaneEventResult::Handled;
                        }
                        _ => {}
                    }
                }

                // Shift+PageUp/PageDown scrolls history
                if modifiers.contains(KeyModifiers::SHIFT) {
                    match key {
                        KeyCode::PageUp => {
                            self.scroll_up(20);
                            return PaneEventResult::Handled;
                        }
                        KeyCode::PageDown => {
                            self.scroll_down(20);
                            return PaneEventResult::Handled;
                        }
                        KeyCode::Up => {
                            self.scroll_up(1);
                            return PaneEventResult::Handled;
                        }
                        KeyCode::Down => {
                            self.scroll_down(1);
                            return PaneEventResult::Handled;
                        }
                        KeyCode::Home => {
                            self.scroll_offset = 0;
                            return PaneEventResult::Handled;
                        }
                        KeyCode::End => {
                            self.scroll_offset = usize::MAX; // Will be clamped
                            return PaneEventResult::Handled;
                        }
                        _ => {}
                    }
                }

                // Forward other keys to PTY
                self.handle_pty_key(key, modifiers)
            }
            PaneEvent::Resized { .. } => PaneEventResult::NotHandled,
        }
    }
}
