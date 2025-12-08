//! Ratatui-based input backend for brush shell.

use std::io::{BufWriter, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use brush_interactive::{InputBackend, InteractivePrompt, ReadResult, ShellError, ShellRef};
use bytes::Bytes;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc::{Sender, channel};
use tui_term::widget::PseudoTerminal;

/// Which pane currently has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedPane {
    /// Terminal output pane is focused
    Terminal,
    /// Command input pane is focused
    CommandInput,
}

/// Ratatui-based input backend that displays terminal output in a tui-term widget
/// and accepts command input in a bottom pane.
pub struct RatatuiInputBackend {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// VT100 parser for the PTY output
    parser: Arc<RwLock<vt100::Parser>>,
    /// Sender for writing to the PTY
    _pty_writer: Sender<Bytes>,
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
}

impl RatatuiInputBackend {
    /// Creates a new `RatatuiInputBackend` with a persistent PTY.
    pub fn new() -> Result<Self, ShellError> {
        // Initialize the ratatui terminal
        let terminal = ratatui::init();

        let terminal_size = terminal.size().map_err(ShellError::IoError)?;

        // Create a PTY using libc directly so we can keep both master and slave fds
        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        let winsize = libc::winsize {
            ws_row: terminal_size.height.saturating_sub(3),
            ws_col: terminal_size.width,
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
            return Err(ShellError::IoError(std::io::Error::last_os_error()));
        }

        // Set close-on-exec for both fds
        // SAFETY: Set close-on-exec flag for both master and slave fds
        unsafe {
            let flags = libc::fcntl(master_fd, libc::F_GETFD);
            libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            let flags = libc::fcntl(slave_fd, libc::F_GETFD);
            libc::fcntl(slave_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }

        // Set up the VT100 parser
        let parser = Arc::new(RwLock::new(vt100::Parser::new(
            terminal_size.height.saturating_sub(3),
            terminal_size.width,
            0,
        )));

        // Spawn a thread to read from PTY master and update the parser
        {
            let parser = Arc::clone(&parser);
            // SAFETY: Duplicate master fd for reading
            let reader_fd = unsafe { libc::dup(master_fd) };
            if reader_fd < 0 {
                return Err(ShellError::IoError(std::io::Error::last_os_error()));
            }
            // SAFETY: We own reader_fd from the successful dup() call above
            let mut reader = unsafe { std::fs::File::from_raw_fd(reader_fd) };

            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
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
            return Err(ShellError::IoError(std::io::Error::last_os_error()));
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
            _pty_writer: tx,
            pty_stdin: slave_stdin,
            pty_stdout: slave_stdout,
            pty_stderr: slave_stderr,
            input_buffer: String::new(),
            cursor_pos: 0,
            focused_pane: FocusedPane::CommandInput,
        })
    }

    /// Draws the UI with the terminal pane and command input pane.
    fn draw_ui(&mut self, _prompt: &InteractivePrompt) -> Result<(), ShellError> {
        let screen = {
            let parser = self
                .parser
                .read()
                .map_err(|_| ShellError::IoError(std::io::Error::other("Failed to lock parser")))?;
            parser.screen().clone()
        };
        let input_buffer = self.input_buffer.clone();
        let cursor_pos = self.cursor_pos;
        let focused_pane = self.focused_pane;

        self.terminal
            .draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(80), // Terminal output pane
                        Constraint::Percentage(20), // Command input pane
                    ])
                    .split(f.area());

                // Render the terminal pane with tui-term
                let (term_title, term_border_style) = if focused_pane == FocusedPane::Terminal {
                    (
                        "Terminal [FOCUSED - Tab to switch]",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("Terminal", Style::default().fg(Color::DarkGray))
                };
                let term_block = Block::default()
                    .borders(Borders::ALL)
                    .title(term_title)
                    .border_style(term_border_style);

                let pseudo_term = PseudoTerminal::new(&screen).block(term_block);
                f.render_widget(pseudo_term, chunks[0]);

                // Render the command input pane
                let (input_title, input_border_style) = if focused_pane == FocusedPane::CommandInput
                {
                    (
                        "Command Input [FOCUSED - Tab to switch]",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("Command Input", Style::default().fg(Color::DarkGray))
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
            })
            .map_err(ShellError::IoError)?;

        Ok(())
    }

    /// Handles keyboard input and returns when Enter is pressed.
    #[allow(clippy::string_slice, clippy::map_unwrap_or)]
    fn handle_events(&mut self) -> Result<Option<ReadResult>, ShellError> {
        if event::poll(Duration::from_millis(50)).map_err(ShellError::IoError)? {
            match event::read().map_err(ShellError::IoError)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Tab => {
                        // Switch focus between panes
                        self.focused_pane = match self.focused_pane {
                            FocusedPane::Terminal => FocusedPane::CommandInput,
                            FocusedPane::CommandInput => FocusedPane::Terminal,
                        };
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(Some(ReadResult::Interrupted));
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if self.input_buffer.is_empty() {
                            return Ok(Some(ReadResult::Eof));
                        }
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
                        return Ok(Some(ReadResult::Input(input)));
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

        Ok(None)
    }
}

impl InputBackend for RatatuiInputBackend {
    fn read_line(
        &mut self,
        _shell: &ShellRef,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        // Event loop: draw UI and handle input until Enter is pressed
        loop {
            self.draw_ui(&prompt)?;

            if let Some(result) = self.handle_events()? {
                return Ok(result);
            }
        }
    }

    fn get_read_buffer(&self) -> Option<(String, usize)> {
        if self.input_buffer.is_empty() {
            None
        } else {
            Some((self.input_buffer.clone(), self.cursor_pos))
        }
    }

    fn set_read_buffer(&mut self, buffer: String, cursor: usize) {
        self.input_buffer = buffer;
        self.cursor_pos = cursor.min(self.input_buffer.len());
    }
}

impl Drop for RatatuiInputBackend {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
