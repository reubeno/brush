//! Ratatui-based input backend for brush shell.

use std::io::{BufWriter, Read, Write};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use brush_interactive::{InputBackend, InteractivePrompt, ReadResult, ShellError, ShellRef};
use bytes::Bytes;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use portable_pty::{PtySize, native_pty_system};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc::{Sender, channel};
use tui_term::widget::PseudoTerminal;

/// Ratatui-based input backend that displays terminal output in a tui-term widget
/// and accepts command input in a bottom pane.
pub struct RatatuiInputBackend {
    /// The ratatui terminal instance
    terminal: DefaultTerminal,
    /// VT100 parser for the PTY output
    parser: Arc<RwLock<vt100::Parser>>,
    /// Sender for writing to the PTY
    _pty_writer: Sender<Bytes>,
    /// Current input buffer
    input_buffer: String,
    /// Cursor position in input buffer
    cursor_pos: usize,
}

impl RatatuiInputBackend {
    /// Creates a new RatatuiInputBackend with a persistent PTY.
    pub fn new() -> Result<Self, ShellError> {
        // Initialize the ratatui terminal
        let terminal = ratatui::init();

        let terminal_size = terminal.size().map_err(|e| ShellError::IoError(e))?;

        // Create a PTY for the shell's I/O
        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: terminal_size.height.saturating_sub(3), // Leave room for command pane
                cols: terminal_size.width,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                ShellError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create PTY: {}", e),
                ))
            })?;

        // Set up the VT100 parser
        let parser = Arc::new(RwLock::new(vt100::Parser::new(
            terminal_size.height.saturating_sub(3),
            terminal_size.width,
            0,
        )));

        // Spawn a thread to read from PTY and update the parser
        {
            let parser = Arc::clone(&parser);
            let mut reader = pty_pair.master.try_clone_reader().map_err(|e| {
                ShellError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to clone PTY reader: {}", e),
                ))
            })?;

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
        let mut writer = BufWriter::new(pty_pair.master.take_writer().map_err(|e| {
            ShellError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to take PTY writer: {}", e),
            ))
        })?);

        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                let _ = writer.write_all(&bytes);
                let _ = writer.flush();
            }
        });

        Ok(Self {
            terminal,
            parser,
            _pty_writer: tx,
            input_buffer: String::new(),
            cursor_pos: 0,
        })
    }

    /// Draws the UI with the terminal pane and command input pane.
    fn draw_ui(&mut self, _prompt: &InteractivePrompt) -> Result<(), ShellError> {
        let parser = self.parser.read().unwrap();
        let screen = parser.screen();
        let input_buffer = self.input_buffer.clone();

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
                let term_block = Block::default()
                    .borders(Borders::ALL)
                    .title("Terminal")
                    .style(Style::default());
                let pseudo_term = PseudoTerminal::new(screen).block(term_block);
                f.render_widget(pseudo_term, chunks[0]);

                // Render the command input pane
                let input_text = format!("> {}", input_buffer);
                let input_paragraph = Paragraph::new(input_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Command Input"),
                    )
                    .style(Style::default());
                f.render_widget(input_paragraph, chunks[1]);
            })
            .map_err(|e| ShellError::IoError(e))?;

        Ok(())
    }

    /// Handles keyboard input and returns when Enter is pressed.
    fn handle_events(&mut self) -> Result<Option<ReadResult>, ShellError> {
        if event::poll(Duration::from_millis(50)).map_err(ShellError::IoError)? {
            match event::read().map_err(ShellError::IoError)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(Some(ReadResult::Interrupted));
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if self.input_buffer.is_empty() {
                            return Ok(Some(ReadResult::Eof));
                        }
                    }
                    KeyCode::Char(c) => {
                        self.input_buffer.insert(self.cursor_pos, c);
                        self.cursor_pos += c.len_utf8();
                    }
                    KeyCode::Backspace => {
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
                    KeyCode::Delete => {
                        if self.cursor_pos < self.input_buffer.len() {
                            self.input_buffer.remove(self.cursor_pos);
                        }
                    }
                    KeyCode::Left => {
                        if self.cursor_pos > 0 {
                            let prev_pos = self.input_buffer[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.cursor_pos = prev_pos;
                        }
                    }
                    KeyCode::Right => {
                        if self.cursor_pos < self.input_buffer.len() {
                            let next_pos = self.input_buffer[self.cursor_pos..]
                                .char_indices()
                                .nth(1)
                                .map(|(i, _)| self.cursor_pos + i)
                                .unwrap_or(self.input_buffer.len());
                            self.cursor_pos = next_pos;
                        }
                    }
                    KeyCode::Home => {
                        self.cursor_pos = 0;
                    }
                    KeyCode::End => {
                        self.cursor_pos = self.input_buffer.len();
                    }
                    KeyCode::Enter => {
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
