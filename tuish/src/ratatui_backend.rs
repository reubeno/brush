//! Ratatui-based TUI for tuish shell.

use std::io::{BufWriter, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
};
use tokio::sync::mpsc::channel;

use crate::content_pane::ContentPane;
use crate::environment_pane::EnvironmentPane;
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
pub struct RatatuiInputBackend {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// Content panes displayed in tabs
    panes: Vec<Box<dyn ContentPane>>,
    /// PTY slave for shell stdin
    pub pty_stdin: std::fs::File,
    /// PTY slave for shell stdout
    pub pty_stdout: std::fs::File,
    /// PTY slave for shell stderr
    pub pty_stderr: std::fs::File,
    /// Current input buffer
    input_buffer: String,
    /// Cursor position in input buffer
    cursor_pos: usize,
    /// Which area currently has focus
    focused_area: FocusedArea,
}

impl RatatuiInputBackend {
    /// Creates a new `RatatuiInputBackend` with a persistent PTY.
    pub fn new() -> Result<Self, std::io::Error> {
        // Initialize the ratatui terminal in raw mode
        let terminal = ratatui::init();

        let terminal_size = terminal.size()?;

        // Create a PTY using libc directly so we can keep both master and slave fds
        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        // PTY dimensions: The top area (80% of screen) contains tabs + bordered content.
        // Tabs bar: 1 line
        // Content border: 2 lines (top + bottom)
        // Additionally, tui-term appears to not use the last row for content, so subtract 1 more.
        let terminal_pane_height = (terminal_size.height * 80) / 100;
        let pty_rows = terminal_pane_height
            .saturating_sub(1) // Tabs bar
            .saturating_sub(2) // Content border
            .saturating_sub(1); // tui-term quirk
        let pty_cols = terminal_size.width.saturating_sub(2); // Content left + right borders
        let winsize = libc::winsize {
            ws_row: pty_rows,
            ws_col: pty_cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // SAFETY: openpty is called with valid pointers to uninitialized integers for fds,
        // null pointers for termios/winsize (using defaults), and a valid winsize struct.
        let result = unsafe {
            libc::openpty(
                std::ptr::addr_of_mut!(master_fd),
                std::ptr::addr_of_mut!(slave_fd),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::addr_of!(winsize).cast_mut(),
            )
        };

        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Set close-on-exec for both fds
        // SAFETY: Set close-on-exec flag for both master and slave fds
        unsafe {
            let flags = libc::fcntl(master_fd, libc::F_GETFD);
            libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            let flags = libc::fcntl(slave_fd, libc::F_GETFD);
            libc::fcntl(slave_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }

        // Set up the VT100 parser with same dimensions as PTY
        let parser = Arc::new(RwLock::new(vt100::Parser::new(pty_rows, pty_cols, 0)));

        // Spawn a thread to read from PTY master and update the parser
        {
            let parser = Arc::clone(&parser);
            // SAFETY: Duplicate master fd for reading
            let reader_fd = unsafe { libc::dup(master_fd) };
            if reader_fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: We own reader_fd from the successful dup() call above
            let mut reader = unsafe { std::fs::File::from_raw_fd(reader_fd) };

            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(size) => {
                            let mut parser = parser.write().unwrap();
                            parser.process(&buf[..size]);
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Set up PTY writer channel
        let (tx, mut rx) = channel::<Bytes>(32);
        // SAFETY: Duplicate master fd for writing
        let writer_fd = unsafe { libc::dup(master_fd) };
        if writer_fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: We own writer_fd from the successful dup() call above
        let writer = unsafe { std::fs::File::from_raw_fd(writer_fd) };
        let mut writer = BufWriter::new(writer);

        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                let _ = writer.write_all(&bytes);
                let _ = writer.flush();
            }
        });

        // Create File handles for brush from the slave fd
        // Duplicate the slave fd three times for stdin, stdout, stderr
        // SAFETY: Duplicate slave fd for stdin
        let slave_stdin = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Duplicate slave fd for stdout
        let slave_stdout = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Duplicate slave fd for stderr
        let slave_stderr = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Close the original slave fd since we've duplicated it three times
        unsafe {
            libc::close(slave_fd);
        }

        // Create content panes
        let panes: Vec<Box<dyn ContentPane>> = vec![
            Box::new(TerminalPane::new(
                Arc::clone(&parser),
                tx,
            )),
            Box::new(EnvironmentPane::new()),
        ];

        Ok(Self {
            terminal,
            panes,
            pty_stdin: slave_stdin,
            pty_stdout: slave_stdout,
            pty_stderr: slave_stderr,
            input_buffer: String::new(),
            cursor_pos: 0,
            focused_area: FocusedArea::CommandInput,
        })
    }

    /// Gets mutable access to a specific pane.
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
    pub fn draw_ui(&mut self) -> Result<(), std::io::Error> {
        let input_buffer = self.input_buffer.clone();
        let cursor_pos = self.cursor_pos;
        let focused_area = self.focused_area;
        
        // Collect tab titles from panes
        let tab_titles: Vec<String> = self.panes.iter().map(|p| p.name().to_string()).collect();
        
        // Track which pane is selected (for rendering) vs focused (for input)
        let selected_pane_index = match focused_area {
            FocusedArea::Pane(idx) => idx,
            FocusedArea::CommandInput => 0, // Keep showing first pane when command input is focused
        };

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

            // Render the command input pane
            let (input_title, input_border_style) = if matches!(focused_area, FocusedArea::CommandInput) {
                (
                    "Command Input [FOCUSED - Ctrl+Space to switch, Ctrl+Q to quit]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "Command Input [Ctrl+Space to focus]",
                    Style::default().fg(Color::DarkGray),
                )
            };
            let input_text = format!("> {input_buffer}");
            let input_paragraph = Paragraph::new(input_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(input_title)
                        .border_style(input_border_style),
                )
                .style(Style::default());
            f.render_widget(input_paragraph, chunks[1]);

            // Render cursor in command input pane when focused
            if matches!(focused_area, FocusedArea::CommandInput) {
                // Cursor position: "> " = 2 chars + border = 1, so x = 3 + cursor_pos
                // y position: top border = 1
                let cursor_x = chunks[1].x + 3 + u16::try_from(cursor_pos).unwrap_or(0);
                let cursor_y = chunks[1].y + 1;
                f.set_cursor_position((cursor_x, cursor_y));
            }
        })?;

        Ok(())
    }

    /// Handles keyboard input and returns Some(command) when Enter is pressed in command pane.
    /// Returns None to signal application shutdown (Ctrl+Q).
    #[allow(clippy::string_slice, clippy::map_unwrap_or, clippy::too_many_lines)]
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
                    KeyCode::Char(c) if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        self.input_buffer.insert(self.cursor_pos, c);
                        self.cursor_pos += c.len_utf8();
                    }
                    KeyCode::Backspace if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        if self.cursor_pos > 0 {
                            let prev_pos = self.input_buffer[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.input_buffer.remove(prev_pos);
                            self.cursor_pos = prev_pos;
                        }
                    }
                    KeyCode::Delete if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        if self.cursor_pos < self.input_buffer.len() {
                            self.input_buffer.remove(self.cursor_pos);
                        }
                    }
                    KeyCode::Left if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        if self.cursor_pos > 0 {
                            let prev_pos = self.input_buffer[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.cursor_pos = prev_pos;
                        }
                    }
                    KeyCode::Right if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        if self.cursor_pos < self.input_buffer.len() {
                            let next_pos = self.input_buffer[self.cursor_pos..]
                                .char_indices()
                                .nth(1)
                                .map(|(i, _)| self.cursor_pos + i)
                                .unwrap_or(self.input_buffer.len());
                            self.cursor_pos = next_pos;
                        }
                    }
                    KeyCode::Home if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        self.cursor_pos = 0;
                    }
                    KeyCode::End if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        self.cursor_pos = self.input_buffer.len();
                    }
                    KeyCode::Enter if matches!(self.focused_area, FocusedArea::CommandInput) => {
                        let input = self.input_buffer.clone();
                        self.input_buffer.clear();
                        self.cursor_pos = 0;
                        return Ok(Some(input));
                    }
                    // Delegate all other keys to the focused pane
                    _ if matches!(self.focused_area, FocusedArea::Pane(idx) if idx < self.panes.len()) => {
                        if let FocusedArea::Pane(idx) = self.focused_area {
                            if let Some(pane) = self.panes.get_mut(idx) {
                                use crate::content_pane::{PaneEvent, PaneEventResult};
                                let result = pane.handle_event(PaneEvent::KeyPress(key.code, key.modifiers));
                                match result {
                                    PaneEventResult::Handled => {},
                                    PaneEventResult::NotHandled => {},
                                    PaneEventResult::RequestClose => {},
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
}

impl Drop for RatatuiInputBackend {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
