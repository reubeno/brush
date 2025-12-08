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
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs},
};
use tokio::sync::mpsc::{Sender, channel};
use tui_term::widget::PseudoTerminal;

/// Which pane currently has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedPane {
    /// A terminal tab is focused (index of the tab)
    Tab(usize),
    /// Command input pane is focused
    CommandInput,
}

/// Ratatui-based TUI that displays terminal output in a tui-term widget
/// and accepts command input in a bottom pane.
pub struct RatatuiInputBackend {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// VT100 parser for the PTY output
    parser: Arc<RwLock<vt100::Parser>>,
    /// Sender for writing to the PTY
    pty_writer: Sender<Bytes>,
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
    /// Which pane currently has focus
    focused_pane: FocusedPane,
    /// Tab titles
    tab_titles: Vec<String>,
    /// Currently selected tab index
    pub selected_tab: usize,
    /// Scroll offset for environment tab
    env_scroll_offset: usize,
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

        Ok(Self {
            terminal,
            parser,
            pty_writer: tx,
            pty_stdin: slave_stdin,
            pty_stdout: slave_stdout,
            pty_stderr: slave_stderr,
            input_buffer: String::new(),
            cursor_pos: 0,
            focused_pane: FocusedPane::CommandInput,
            tab_titles: vec!["Terminal 1".to_string(), "Environment".to_string()],
            selected_tab: 0,
            env_scroll_offset: 0,
        })
    }

    /// Draws the UI with the terminal pane and command input pane.
    #[allow(clippy::too_many_lines)]
    pub fn draw_ui(&mut self, env_vars: Option<&[(String, String)]>) -> Result<(), std::io::Error> {
        let screen = {
            let parser = self
                .parser
                .read()
                .map_err(|_| std::io::Error::other("Failed to lock parser"))?;
            let screen = parser.screen().clone();
            drop(parser);
            screen
        };
        let input_buffer = self.input_buffer.clone();
        let cursor_pos = self.cursor_pos;
        let focused_pane = self.focused_pane;
        let tab_titles = self.tab_titles.clone();
        let selected_tab = self.selected_tab;
        let env_scroll_offset = self.env_scroll_offset;

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
            let tab_selection = if matches!(focused_pane, FocusedPane::CommandInput) {
                usize::MAX // Invalid index = no selection
            } else {
                selected_tab
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

            // Render content area with borders based on focus and selected tab
            let tab_focused = matches!(focused_pane, FocusedPane::Tab(_));
            let content_border_style = if tab_focused {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Render content based on selected tab
            if selected_tab == 0 {
                // Tab 0: Terminal with bordered block
                let terminal_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(content_border_style);
                let terminal_inner = terminal_block.inner(tab_area_chunks[1]);
                f.render_widget(terminal_block, tab_area_chunks[1]);

                let pseudo_term = PseudoTerminal::new(&screen);
                f.render_widget(pseudo_term, terminal_inner);
            } else if selected_tab == 1 {
                // Tab 1: Environment with bordered block
                let env_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(content_border_style);
                let env_inner = env_block.inner(tab_area_chunks[1]);
                f.render_widget(env_block, tab_area_chunks[1]);

                if let Some(vars) = &env_vars {
                    // Create table with header and rows
                    let header = Row::new(vec![
                        Cell::from("Variable").style(Style::default().add_modifier(Modifier::BOLD)),
                        Cell::from("Value").style(Style::default().add_modifier(Modifier::BOLD)),
                    ])
                    .style(Style::default().bg(Color::DarkGray));

                    // Clamp scroll offset to prevent scrolling past content
                    let available_height = env_inner.height.saturating_sub(2); // Subtract header and bottom margin
                    let max_scroll = vars.len().saturating_sub(available_height as usize);
                    let clamped_scroll = env_scroll_offset.min(max_scroll);

                    // Skip rows based on scroll offset
                    let rows = vars.iter().skip(clamped_scroll).map(|(k, v)| {
                        Row::new(vec![
                            Cell::from(k.as_str())
                                .style(Style::default().add_modifier(Modifier::ITALIC)),
                            Cell::from(v.as_str()),
                        ])
                    });

                    let table = Table::new(
                        rows,
                        [Constraint::Percentage(30), Constraint::Percentage(70)],
                    )
                    .header(header)
                    .style(Style::default().fg(Color::White));

                    f.render_widget(table, env_inner);
                } else {
                    let loading = Paragraph::new("Loading environment variables...")
                        .style(Style::default().fg(Color::White));
                    f.render_widget(loading, env_inner);
                }
            }

            // Render the command input pane
            let (input_title, input_border_style) = if focused_pane == FocusedPane::CommandInput {
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
            if focused_pane == FocusedPane::CommandInput {
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
                    // Ctrl+Space cycles focus through tabs and command input
                    KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.focused_pane = match self.focused_pane {
                            FocusedPane::Tab(idx) => {
                                // Move to next tab, or to command input if at last tab
                                if idx + 1 < self.tab_titles.len() {
                                    self.selected_tab = idx + 1;
                                    FocusedPane::Tab(idx + 1)
                                } else {
                                    FocusedPane::CommandInput
                                }
                            }
                            FocusedPane::CommandInput => {
                                // Cycle back to first tab
                                self.selected_tab = 0;
                                FocusedPane::Tab(0)
                            }
                        };
                    }
                    // Ctrl+Q quits the application by returning None to signal shutdown
                    KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    // Scrolling in environment tab
                    KeyCode::Up if self.focused_pane == FocusedPane::Tab(1) => {
                        self.env_scroll_offset = self.env_scroll_offset.saturating_sub(1);
                    }
                    KeyCode::Down if self.focused_pane == FocusedPane::Tab(1) => {
                        self.env_scroll_offset = self.env_scroll_offset.saturating_add(1);
                    }
                    KeyCode::PageUp if self.focused_pane == FocusedPane::Tab(1) => {
                        self.env_scroll_offset = self.env_scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown if self.focused_pane == FocusedPane::Tab(1) => {
                        self.env_scroll_offset = self.env_scroll_offset.saturating_add(10);
                    }
                    KeyCode::Char('c')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && matches!(self.focused_pane, FocusedPane::Tab(_)) =>
                    {
                        // Send Ctrl+C to PTY when in Terminal pane
                        let _ = self.pty_writer.try_send(Bytes::from(vec![0x03]));
                    }
                    KeyCode::Char('d')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && matches!(self.focused_pane, FocusedPane::Tab(_)) =>
                    {
                        // Send Ctrl+D to PTY when in Terminal pane
                        let _ = self.pty_writer.try_send(Bytes::from(vec![0x04]));
                    }
                    KeyCode::Char(c) if self.focused_pane == FocusedPane::CommandInput => {
                        self.input_buffer.insert(self.cursor_pos, c);
                        self.cursor_pos += c.len_utf8();
                    }
                    KeyCode::Backspace if self.focused_pane == FocusedPane::CommandInput => {
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
                    KeyCode::Delete if self.focused_pane == FocusedPane::CommandInput => {
                        if self.cursor_pos < self.input_buffer.len() {
                            self.input_buffer.remove(self.cursor_pos);
                        }
                    }
                    KeyCode::Left if self.focused_pane == FocusedPane::CommandInput => {
                        if self.cursor_pos > 0 {
                            let prev_pos = self.input_buffer[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.cursor_pos = prev_pos;
                        }
                    }
                    KeyCode::Right if self.focused_pane == FocusedPane::CommandInput => {
                        if self.cursor_pos < self.input_buffer.len() {
                            let next_pos = self.input_buffer[self.cursor_pos..]
                                .char_indices()
                                .nth(1)
                                .map(|(i, _)| self.cursor_pos + i)
                                .unwrap_or(self.input_buffer.len());
                            self.cursor_pos = next_pos;
                        }
                    }
                    KeyCode::Home if self.focused_pane == FocusedPane::CommandInput => {
                        self.cursor_pos = 0;
                    }
                    KeyCode::End if self.focused_pane == FocusedPane::CommandInput => {
                        self.cursor_pos = self.input_buffer.len();
                    }
                    KeyCode::Enter if self.focused_pane == FocusedPane::CommandInput => {
                        let input = self.input_buffer.clone();
                        self.input_buffer.clear();
                        self.cursor_pos = 0;
                        return Ok(Some(input));
                    }
                    // Forward keyboard input to PTY when a tab is focused
                    KeyCode::Char(c) if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        // Handle Ctrl+key combinations
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            // Ctrl+A through Ctrl+Z map to 0x01-0x1A
                            if c.is_ascii_alphabetic() {
                                let ctrl_code = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                                let _ = self.pty_writer.try_send(Bytes::from(vec![ctrl_code]));
                            } else {
                                // Other Ctrl combinations, send as-is
                                let _ = self.pty_writer.try_send(Bytes::from(c.to_string()));
                            }
                        } else {
                            let _ = self.pty_writer.try_send(Bytes::from(c.to_string()));
                        }
                    }
                    KeyCode::Tab if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(vec![b'\t']));
                    }
                    KeyCode::Enter if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(vec![b'\r']));
                    }
                    KeyCode::Backspace if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(vec![0x7f]));
                    }
                    KeyCode::Esc if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(vec![0x1b]));
                    }
                    KeyCode::Delete if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[3~".as_slice()));
                    }
                    KeyCode::Insert if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[2~".as_slice()));
                    }
                    KeyCode::Home if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[H".as_slice()));
                    }
                    KeyCode::End if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[F".as_slice()));
                    }
                    KeyCode::PageUp if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[5~".as_slice()));
                    }
                    KeyCode::PageDown if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[6~".as_slice()));
                    }
                    KeyCode::Up if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[A".as_slice()));
                    }
                    KeyCode::Down if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[B".as_slice()));
                    }
                    KeyCode::Right if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[C".as_slice()));
                    }
                    KeyCode::Left if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        let _ = self.pty_writer.try_send(Bytes::from(b"\x1b[D".as_slice()));
                    }
                    KeyCode::F(n) if matches!(self.focused_pane, FocusedPane::Tab(_)) => {
                        // F1-F12 function keys
                        let seq = match n {
                            1 => b"\x1bOP".as_slice(),
                            2 => b"\x1bOQ".as_slice(),
                            3 => b"\x1bOR".as_slice(),
                            4 => b"\x1bOS".as_slice(),
                            5 => b"\x1b[15~".as_slice(),
                            6 => b"\x1b[17~".as_slice(),
                            7 => b"\x1b[18~".as_slice(),
                            8 => b"\x1b[19~".as_slice(),
                            9 => b"\x1b[20~".as_slice(),
                            10 => b"\x1b[21~".as_slice(),
                            11 => b"\x1b[23~".as_slice(),
                            12 => b"\x1b[24~".as_slice(),
                            _ => b"".as_slice(),
                        };
                        if !seq.is_empty() {
                            let _ = self.pty_writer.try_send(Bytes::from(seq));
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
