//! Terminal content pane with command-grouped history.
//!
//! This module provides a terminal pane that displays command output in discrete,
//! navigable blocks. All output (completed and in-progress) is shown in a unified
//! scrollable view.

use std::sync::{Arc, RwLock};

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
        let mut count = self.lines.len();
        for line in self.lines.iter().rev() {
            if line.spans.is_empty() || line.spans.iter().all(|s| s.content.trim().is_empty()) {
                count = count.saturating_sub(1);
            } else {
                break;
            }
        }
        count.max(1)
    }
}

/// A content pane that displays PTY terminal output with command-grouped history.
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
    last_pty_dimensions: (u16, u16),
    /// Scroll offset (in lines) for viewing history. 0 = viewing from top.
    /// `usize::MAX` is a sentinel for "auto-scroll to bottom".
    scroll_offset: usize,
    /// Cached total line count from last render (used to normalize scroll offset).
    last_total_lines: usize,
    /// Cached visible height from last render.
    last_visible_height: usize,
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
            last_pty_dimensions: (0, 0),
            scroll_offset: usize::MAX, // Start scrolled to bottom
            last_total_lines: 0,
            last_visible_height: 40, // Reasonable default
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
        // Auto-scroll to bottom
        self.scroll_offset = usize::MAX;
    }

    /// Captures styled output from the vt100 parser using cell-by-cell rendering.
    #[allow(clippy::significant_drop_tightening)]
    fn capture_styled_output(parser: &Arc<RwLock<vt100::Parser>>) -> Vec<Line<'static>> {
        let parser_guard = parser.read().unwrap();
        let screen = parser_guard.screen();
        let size = screen.size();
        let (rows, cols) = (size.0 as usize, size.1 as usize);

        // Theme background - use this when cell has default/black background
        let theme_bg = Color::Rgb(22, 22, 30);

        let mut lines = Vec::with_capacity(rows);

        for row in 0..rows {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut current_text = String::new();
            let mut current_style: Option<Style> = None;

            for col in 0..cols {
                let cell = screen.cell(row as u16, col as u16);
                let cell = match cell {
                    Some(c) => c,
                    None => continue,
                };

                let ch = cell.contents();
                let vt_fg = cell.fgcolor();
                let vt_bg = cell.bgcolor();

                // Convert vt100 colors to ratatui colors
                let fg = Self::convert_vt100_color(vt_fg);
                let bg = Self::convert_vt100_color(vt_bg);

                // Replace default/black background with theme background
                let bg = match bg {
                    Color::Black | Color::Reset => theme_bg,
                    other => other,
                };

                let mut style = Style::default().fg(fg).bg(bg);

                // Apply text modifiers
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                // Check if style changed
                if current_style != Some(style) {
                    // Flush accumulated text
                    if !current_text.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut current_text),
                            current_style.unwrap_or_default(),
                        ));
                    }
                    current_style = Some(style);
                }

                // Add character (use space if empty)
                if ch.is_empty() {
                    current_text.push(' ');
                } else {
                    current_text.push_str(&ch);
                }
            }

            // Flush final span
            if !current_text.is_empty() {
                spans.push(Span::styled(
                    current_text,
                    current_style.unwrap_or_default(),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    /// Captures the current live terminal output as styled lines.
    /// Uses cell-by-cell rendering to preserve spacing and handle backgrounds correctly.
    #[allow(clippy::significant_drop_tightening)]
    fn capture_live_output(&self) -> Vec<Line<'static>> {
        let parser_guard = self.shared_parser.read().unwrap();
        let screen = parser_guard.screen();
        let size = screen.size();
        let (rows, cols) = (size.0 as usize, size.1 as usize);

        // Theme background - use this when cell has default/black background
        let theme_bg = Color::Rgb(22, 22, 30);

        let mut lines = Vec::with_capacity(rows);

        for row in 0..rows {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut current_text = String::new();
            let mut current_style: Option<Style> = None;

            for col in 0..cols {
                let cell = screen.cell(row as u16, col as u16);
                let cell = match cell {
                    Some(c) => c,
                    None => continue,
                };

                let ch = cell.contents();
                let vt_fg = cell.fgcolor();
                let vt_bg = cell.bgcolor();

                // Convert vt100 colors to ratatui colors
                let fg = Self::convert_vt100_color(vt_fg);
                let bg = Self::convert_vt100_color(vt_bg);

                // Replace default/black background with theme background
                let bg = match bg {
                    Color::Black | Color::Reset => theme_bg,
                    other => other,
                };

                let mut style = Style::default().fg(fg).bg(bg);

                // Apply text modifiers
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                // Check if style changed
                if current_style != Some(style) {
                    // Flush accumulated text
                    if !current_text.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut current_text),
                            current_style.unwrap_or_default(),
                        ));
                    }
                    current_style = Some(style);
                }

                // Add character (use space if empty)
                if ch.is_empty() {
                    current_text.push(' ');
                } else {
                    current_text.push_str(&ch);
                }
            }

            // Flush final span
            if !current_text.is_empty() {
                spans.push(Span::styled(
                    current_text,
                    current_style.unwrap_or_default(),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    /// Converts a vt100 color to a ratatui color.
    fn convert_vt100_color(color: vt100::Color) -> Color {
        match color {
            vt100::Color::Default => Color::Reset,
            vt100::Color::Idx(0) => Color::Black,
            vt100::Color::Idx(1) => Color::Red,
            vt100::Color::Idx(2) => Color::Green,
            vt100::Color::Idx(3) => Color::Yellow,
            vt100::Color::Idx(4) => Color::Blue,
            vt100::Color::Idx(5) => Color::Magenta,
            vt100::Color::Idx(6) => Color::Cyan,
            vt100::Color::Idx(7) => Color::White,
            vt100::Color::Idx(8) => Color::DarkGray,
            vt100::Color::Idx(9) => Color::LightRed,
            vt100::Color::Idx(10) => Color::LightGreen,
            vt100::Color::Idx(11) => Color::LightYellow,
            vt100::Color::Idx(12) => Color::LightBlue,
            vt100::Color::Idx(13) => Color::LightMagenta,
            vt100::Color::Idx(14) => Color::LightCyan,
            vt100::Color::Idx(15) => Color::White,
            vt100::Color::Idx(idx) => Color::Indexed(idx),
            vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
        }
    }

    /// Sends raw bytes to the PTY.
    fn send_to_pty(&self, bytes: impl AsRef<[u8]>) {
        let _ = self
            .pty_writer
            .try_send(Bytes::copy_from_slice(bytes.as_ref()));
    }

    /// Processes output from the PTY (for compatibility).
    #[allow(dead_code)]
    pub fn process_output(&self, data: &[u8]) {
        let mut parser = self.shared_parser.write().unwrap();
        parser.process(data);
    }

    /// Returns whether we're in alternate screen mode.
    fn is_alternate_screen(&self) -> bool {
        let parser = self.shared_parser.read().unwrap();
        parser.screen().alternate_screen()
    }

    /// Ensures PTY is sized to the given dimensions.
    fn ensure_pty_size(&mut self, height: u16, width: u16) {
        let new_dimensions = (height, width);
        if new_dimensions != self.last_pty_dimensions && height > 0 && width > 0 {
            if let Err(e) = self.pty_handle.resize(height, width) {
                tracing::warn!("Failed to resize PTY: {}", e);
            }
            self.last_pty_dimensions = new_dimensions;
        }
    }

    /// Renders a completed block header - clean, integrated style.
    fn render_block_header(block: &CommandOutputBlock, _width: u16) -> Line<'static> {
        // Theme colors
        let text_primary = Color::Rgb(220, 220, 235);
        let text_muted = Color::Rgb(100, 100, 120);

        let (status_char, status_color) = match block.exit_code {
            Some(0) => ("●", Color::Rgb(134, 239, 172)), // Green dot
            Some(_) => ("●", Color::Rgb(248, 113, 113)), // Red dot
            None => ("○", Color::Rgb(250, 204, 21)),     // Yellow hollow
        };

        let cmd_display: String = if block.command.chars().count() > MAX_COMMAND_DISPLAY_LENGTH {
            let truncated: String = block
                .command
                .chars()
                .take(MAX_COMMAND_DISPLAY_LENGTH)
                .collect();
            format!("{truncated}…")
        } else {
            block.command.clone()
        };

        let timestamp = block.timestamp.format("%H:%M:%S").to_string();

        // Clean format: status dot, command, timestamp - no horizontal lines
        Line::from(vec![
            Span::styled(format!("{status_char} "), Style::default().fg(status_color)),
            Span::styled(
                cmd_display,
                Style::default()
                    .fg(text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {timestamp}"), Style::default().fg(text_muted)),
        ])
    }

    /// Renders a "running" block header for the current command.
    fn render_running_header(command: &str, _width: u16) -> Line<'static> {
        let accent = Color::Rgb(139, 92, 246); // Purple
        let text_primary = Color::Rgb(220, 220, 235);

        let cmd_display: String = if command.chars().count() > MAX_COMMAND_DISPLAY_LENGTH {
            let truncated: String = command.chars().take(MAX_COMMAND_DISPLAY_LENGTH).collect();
            format!("{truncated}…")
        } else {
            command.to_string()
        };

        // Pulsing indicator style for running command
        Line::from(vec![
            Span::styled("◉ ", Style::default().fg(accent)),
            Span::styled(
                cmd_display,
                Style::default()
                    .fg(text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  running", Style::default().fg(accent)),
        ])
    }

    /// Builds all lines for the unified scrollable view.
    fn build_all_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        let separator_color = Color::Rgb(45, 45, 60);

        // Add completed history blocks
        for (i, block) in self.history.iter().enumerate() {
            // Add thin separator line between blocks (not before first)
            if i > 0 {
                let separator_line = Line::from(Span::styled(
                    "─".repeat(width as usize),
                    Style::default().fg(separator_color),
                ));
                all_lines.push(separator_line);
            }

            all_lines.push(Self::render_block_header(block, width));
            let content_count = block.content_line_count();
            for line in block.lines.iter().take(content_count) {
                all_lines.push(line.clone());
            }
        }

        // Add running command output if any
        if let Some(ref cmd) = self.running_command {
            // Separator before running command if there's history
            if !self.history.is_empty() {
                let separator_line = Line::from(Span::styled(
                    "─".repeat(width as usize),
                    Style::default().fg(separator_color),
                ));
                all_lines.push(separator_line);
            }

            all_lines.push(Self::render_running_header(cmd, width));
            let live_lines = self.capture_live_output();
            // Trim trailing empty lines from live output
            let mut live_count = live_lines.len();
            for line in live_lines.iter().rev() {
                if line.spans.is_empty() || line.spans.iter().all(|s| s.content.trim().is_empty()) {
                    live_count = live_count.saturating_sub(1);
                } else {
                    break;
                }
            }
            for line in live_lines.into_iter().take(live_count.max(1)) {
                all_lines.push(line);
            }
        }

        all_lines
    }

    /// Renders the unified scrollable view of all blocks.
    fn render_unified_view(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Always keep PTY sized to full area (for apps like nvim that may use alternate screen)
        self.ensure_pty_size(area.height, area.width);

        let all_lines = self.build_all_lines(area.width);
        let total_lines = all_lines.len();
        let visible_height = area.height as usize;

        // Cache dimensions for scroll calculations
        self.last_total_lines = total_lines;
        self.last_visible_height = visible_height;

        // Clamp scroll offset
        let max_scroll = total_lines.saturating_sub(visible_height);
        let effective_scroll = self.scroll_offset.min(max_scroll);

        // Get visible slice
        let visible_lines: Vec<Line<'static>> = all_lines
            .into_iter()
            .skip(effective_scroll)
            .take(visible_height)
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        frame.render_widget(paragraph, area);

        // Render scrollbar if content overflows
        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(total_lines)
                .position(effective_scroll);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Rgb(100, 100, 130)));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    /// Renders the terminal fullscreen for alternate screen mode (vim, nvim, etc.).
    #[allow(clippy::significant_drop_tightening)]
    fn render_fullscreen_terminal(&mut self, frame: &mut Frame<'_>, area: Rect) {
        // Ensure PTY is sized to full area
        self.ensure_pty_size(area.height, area.width);

        let parser = self.shared_parser.read().unwrap();
        let screen = parser.screen();
        let cursor = tui_term::widget::Cursor::default().visibility(self.is_focused);
        let pseudo_term = PseudoTerminal::new(screen).cursor(cursor);
        frame.render_widget(pseudo_term, area);
    }

    /// Scrolls up by the given number of lines.
    fn scroll_up(&mut self, lines: usize) {
        // If at auto-scroll sentinel (usize::MAX), normalize to actual max first
        if self.scroll_offset == usize::MAX {
            // Compute actual max scroll position from cached values
            self.scroll_offset = self
                .last_total_lines
                .saturating_sub(self.last_visible_height);
        }
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scrolls down by the given number of lines.
    fn scroll_down(&mut self, lines: usize) {
        // If at auto-scroll sentinel, scrolling down is a no-op (already at bottom)
        if self.scroll_offset == usize::MAX {
            return;
        }
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

        // Normalize scroll_offset from sentinel value
        let effective_scroll = if self.scroll_offset == usize::MAX {
            self.last_total_lines
                .saturating_sub(self.last_visible_height)
        } else {
            self.scroll_offset
        };

        // Find which block we're currently viewing
        let mut current_block = 0;
        let mut offset = 0;
        for (idx, block) in self.history.iter().enumerate() {
            let block_end = offset + 1 + block.content_line_count() + 1;
            if effective_scroll < block_end {
                current_block = idx;
                break;
            }
            offset = block_end;
            current_block = idx;
        }
        // Jump to previous block (or start of current if not at start)
        if effective_scroll > self.block_start_offset(current_block) {
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

        // Normalize scroll_offset from sentinel value
        let effective_scroll = if self.scroll_offset == usize::MAX {
            self.last_total_lines
                .saturating_sub(self.last_visible_height)
        } else {
            self.scroll_offset
        };

        // Find which block we're currently viewing
        let mut current_block = 0;
        let mut offset = 0;
        for (idx, block) in self.history.iter().enumerate() {
            let block_end = offset + 1 + block.content_line_count() + 1;
            if effective_scroll < block_end {
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
            // At last completed block - jump to running command or end
            self.scroll_offset = usize::MAX;
        }
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
            KeyCode::Char(c) => self.send_to_pty(c.to_string().into_bytes()),
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
        // Consistent theme background
        let bg_base = Color::Rgb(22, 22, 30);
        let bg = ratatui::widgets::Block::default().style(Style::default().bg(bg_base));
        frame.render_widget(bg, area);

        // In alternate screen mode (vim, nvim, etc.), render PTY directly fullscreen
        if self.is_alternate_screen() {
            self.render_fullscreen_terminal(frame, area);
            return;
        }

        // Otherwise, render unified scrollable view of all blocks
        self.render_unified_view(frame, area);
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
                // Ctrl+Shift+Up/Down OR Alt+Up/Down jumps between blocks
                // (Alt variant provided since many terminals intercept Ctrl+Shift+arrows)
                let has_ctrl_shift =
                    modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT);
                let has_alt = modifiers.contains(KeyModifiers::ALT)
                    && !modifiers.contains(KeyModifiers::SHIFT);

                if has_ctrl_shift || has_alt {
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

                // Shift+keys for scrolling - these ALWAYS work regardless of running command
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
                            self.scroll_offset = usize::MAX;
                            return PaneEventResult::Handled;
                        }
                        _ => {} // Other Shift+key combos fall through
                    }
                }

                // If a command is running, forward all other keys to PTY
                if self.running_command.is_some() {
                    return self.handle_pty_key(key, modifiers);
                }

                // No command running - only accept Ctrl+C, reject character input
                // but let navigation keys through
                match key {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        self.send_to_pty(vec![3]); // Ctrl+C
                        PaneEventResult::Handled
                    }
                    // Block character input when idle - let app handle it
                    KeyCode::Char(_) => PaneEventResult::NotHandled,
                    KeyCode::Enter | KeyCode::Backspace | KeyCode::Tab => {
                        PaneEventResult::NotHandled
                    }
                    // Allow other keys (arrows, etc.) to be handled as "we got it but nothing to
                    // do"
                    _ => PaneEventResult::Handled,
                }
            }
            PaneEvent::Resized { .. } => PaneEventResult::NotHandled,
        }
    }
}
