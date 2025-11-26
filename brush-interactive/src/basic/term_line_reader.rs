//
// This module is intentionally limited, and does not have all the bells and whistles. We wan
// enough here that we can use it in the basic shell for (p)expect/pty-style testing of
// completion, and without using VT100-style escape sequences for cursor movement and display.
//

use crossterm::ExecutableCommand;
use std::io::Write;

use crate::{ReadResult, ShellError};

const BACKSPACE: char = 8u8 as char;

pub(crate) struct TermLineReader {
    term_mode: brush_core::terminal::AutoModeGuard,
}

impl TermLineReader {
    pub fn new() -> Result<Self, ShellError> {
        let reader = Self {
            term_mode: brush_core::terminal::AutoModeGuard::new(
                brush_core::openfiles::OpenFile::Stdin(std::io::stdin()),
            )?,
        };

        let settings = brush_core::terminal::Settings::builder()
            .echo_input(false)
            .line_input(false)
            .interrupt_signals(false)
            .output_nl_as_nlcr(true)
            .build();

        reader.term_mode.apply_settings(&settings)?;

        Ok(reader)
    }
}

impl super::LineReader for TermLineReader {
    fn read_line(
        &self,
        prompt: Option<&str>,
        mut completion_handler: impl FnMut(
            &str,
            usize,
        )
            -> Result<brush_core::completion::Completions, ShellError>,
    ) -> Result<ReadResult, ShellError> {
        let mut state = ReadLineState::new(prompt);
        state.display_prompt()?;

        loop {
            if let crossterm::event::Event::Key(event) = crossterm::event::read()? {
                if let Some(result) = state.on_key(event, &mut completion_handler)? {
                    return Ok(result);
                }
            }
        }
    }
}

struct ReadLineState<'a> {
    // Current line of input
    line: String,
    // Current position of cursor, expressed as a byte offset from the
    // start of `line`. We maintain the invariant that it will always
    // be at a clean character boundary.
    cursor: usize,
    // Current prompt to use.
    prompt: Option<&'a str>,
}

impl<'a> ReadLineState<'a> {
    const fn new(prompt: Option<&'a str>) -> Self {
        Self {
            line: String::new(),
            cursor: 0,
            prompt,
        }
    }

    pub fn display_prompt(&self) -> Result<(), ShellError> {
        if let Some(prompt) = self.prompt {
            eprint!("{prompt}");
            std::io::stderr().flush()?;
        }
        Ok(())
    }

    fn on_key(
        &mut self,
        event: crossterm::event::KeyEvent,
        mut completion_handler: impl FnMut(
            &str,
            usize,
        )
            -> Result<brush_core::completion::Completions, ShellError>,
    ) -> Result<Option<ReadResult>, ShellError> {
        match (event.modifiers, event.code) {
            (_, crossterm::event::KeyCode::Enter)
            | (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('j')) => {
                Self::display_newline()?;
                self.line.push('\n');
                let line = std::mem::take(&mut self.line);
                return Ok(Some(ReadResult::Input(line)));
            }
            (
                crossterm::event::KeyModifiers::SHIFT | crossterm::event::KeyModifiers::NONE,
                crossterm::event::KeyCode::Char(c),
            ) => {
                self.on_char(c)?;
            }
            (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('c')) => {
                eprintln!("^C");
                return Ok(Some(ReadResult::Interrupted));
            }
            (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('d')) => {
                if self.line.is_empty() {
                    Self::display_newline()?;
                    return Ok(Some(ReadResult::Eof));
                }
            }
            (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('l')) => {
                self.clear_screen()?;
            }
            (_, crossterm::event::KeyCode::Backspace) => {
                self.backspace()?;
            }
            (_, crossterm::event::KeyCode::Left) => {
                self.move_cursor_left()?;
            }
            (_, crossterm::event::KeyCode::Tab) => {
                let completions = completion_handler(self.line.as_str(), self.cursor)?;
                self.handle_completions(&completions)?;
            }
            _ => (),
        }

        Ok(None)
    }

    fn on_char(&mut self, c: char) -> Result<(), ShellError> {
        self.line.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        eprint!("{c}");
        std::io::stderr().flush()?;

        Ok(())
    }

    fn display_newline() -> Result<(), ShellError> {
        eprintln!();
        std::io::stderr().flush()?;

        Ok(())
    }

    fn clear_screen(&self) -> Result<(), ShellError> {
        std::io::stderr()
            .execute(crossterm::terminal::Clear(
                crossterm::terminal::ClearType::All,
            ))?
            .execute(crossterm::cursor::MoveTo(0, 0))?;

        self.display_prompt()?;
        eprint!("{}", self.line.as_str());
        std::io::stderr().flush()?;
        Ok(())
    }

    #[allow(clippy::string_slice, reason = "it's calculated based on char indices")]
    fn backspace(&mut self) -> Result<(), ShellError> {
        let char_indices = self.line.char_indices();

        let Some((last_char_index, _)) = char_indices.last() else {
            return Ok(());
        };

        self.cursor = last_char_index;
        self.line.truncate(last_char_index);

        eprint!("{BACKSPACE}");
        eprint!("{} ", &self.line[self.cursor..]);
        eprint!(
            "{}",
            repeated_char_str(BACKSPACE, self.line.len() + 1 - self.cursor)
        );

        std::io::stderr().flush()?;
        Ok(())
    }

    fn move_cursor_left(&mut self) -> Result<(), ShellError> {
        eprint!("{BACKSPACE}");
        std::io::stderr().flush()?;

        self.cursor = self.cursor.saturating_sub(1);

        while self.cursor > 0 && !self.line.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }

        Ok(())
    }

    fn handle_completions(
        &mut self,
        completions: &brush_core::completion::Completions,
    ) -> Result<(), ShellError> {
        if completions.candidates.is_empty() {
            // Do nothing
            Ok(())
        } else if completions.candidates.len() == 1 {
            self.handle_single_completion(completions)
        } else {
            self.handle_multiple_completions(completions)
        }
    }

    #[allow(clippy::unwrap_in_result)]
    #[expect(
        clippy::string_slice,
        reason = "all offsets are expected to be at char boundaries"
    )]
    fn handle_single_completion(
        &mut self,
        completions: &brush_core::completion::Completions,
    ) -> Result<(), ShellError> {
        // Apply replacement directly.
        let candidate = completions.candidates.iter().next().unwrap();
        if completions.insertion_index + completions.delete_count != self.cursor {
            return Ok(());
        }

        let mut delete_count = completions.delete_count;
        let mut redisplay_offset = completions.insertion_index;

        // Don't bother erasing and re-writing the portion of the
        // completion's prefix that
        // is identical to what we already had in the token-being-completed.
        if delete_count > 0
            && candidate.starts_with(&self.line[redisplay_offset..redisplay_offset + delete_count])
        {
            redisplay_offset += delete_count;
            delete_count = 0;
        }

        let mut updated_line = self.line.clone();
        updated_line.truncate(completions.insertion_index);
        updated_line.push_str(candidate);
        updated_line.push_str(&self.line[self.cursor..]);
        self.line = updated_line;

        self.cursor = completions.insertion_index + candidate.len();

        let move_left = repeated_char_str(BACKSPACE, delete_count);
        eprint!("{move_left}{}", &self.line[redisplay_offset..]);

        // TODO(completion): Remove trailing chars if completion is shorter?
        eprint!(
            "{}",
            repeated_char_str(BACKSPACE, self.line.len() - self.cursor)
        );

        std::io::stderr().flush()?;

        Ok(())
    }

    fn handle_multiple_completions(
        &self,
        completions: &brush_core::completion::Completions,
    ) -> Result<(), ShellError> {
        // Display replacements.
        Self::display_newline()?;
        for candidate in &completions.candidates {
            let formatted = format_completion_candidate(candidate.as_str(), &completions.options);
            eprintln!("{formatted}");
        }
        std::io::stderr().flush()?;

        // Re-display prompt.
        self.display_prompt()?;

        // Re-display line so far.
        eprint!(
            "{}{}",
            self.line,
            repeated_char_str(BACKSPACE, self.line.len() - self.cursor)
        );

        std::io::stderr().flush()?;

        Ok(())
    }
}

#[allow(clippy::string_slice)]
fn format_completion_candidate(
    mut candidate: &str,
    options: &brush_core::completion::ProcessingOptions,
) -> String {
    if options.treat_as_filenames {
        let trimmed = candidate
            .strip_suffix(std::path::MAIN_SEPARATOR)
            .unwrap_or(candidate);
        if let Some(index) = trimmed.rfind(std::path::MAIN_SEPARATOR) {
            candidate = &candidate[index + std::path::MAIN_SEPARATOR.len_utf8()..];
        }
    }

    candidate.to_string()
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
