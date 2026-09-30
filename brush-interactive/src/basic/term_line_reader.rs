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
            term_mode: brush_core::terminal::AutoModeGuard::new(std::io::stdin().into())?,
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
        mut completion_handler: impl FnMut(&str, usize) -> Result<crate::completion::Offers, ShellError>,
    ) -> Result<ReadResult, ShellError> {
        let mut state = ReadLineState::new(prompt);
        state.display_prompt()?;

        loop {
            if let crossterm::event::Event::Key(event) = crossterm::event::read()?
                && let Some(result) = state.on_key(event, &mut completion_handler)?
            {
                return Ok(result);
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
        mut completion_handler: impl FnMut(&str, usize) -> Result<crate::completion::Offers, ShellError>,
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
            (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('d'))
                if self.line.is_empty() =>
            {
                Self::display_newline()?;
                return Ok(Some(ReadResult::Eof));
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

    fn handle_completions(&mut self, offers: &crate::completion::Offers) -> Result<(), ShellError> {
        match (&offers.edit, offers.list.as_slice()) {
            (None, []) => Ok(()),
            (Some(edit), []) => self.handle_single_completion(edit),
            (edit, list) => {
                // Like readline, an edit made along with listing candidates shows when the
                // line is redrawn after them.
                if let Some(edit) = edit {
                    self.apply_edit(edit);
                }
                self.handle_multiple_completions(list)
            }
        }
    }

    /// Applies `edit` to the line, without showing it; returns false if it doesn't apply at
    /// the cursor.
    fn apply_edit(&mut self, edit: &brush_core::completion::Edit) -> bool {
        let replace = &edit.replace;
        if !(replace.start <= self.cursor && self.cursor <= replace.end)
            || replace.end > self.line.len()
        {
            return false;
        }

        self.line.replace_range(replace.clone(), &edit.text);
        self.cursor = replace.start + edit.text.len();
        true
    }

    #[expect(
        clippy::string_slice,
        reason = "all offsets are expected to be at char boundaries"
    )]
    fn handle_single_completion(
        &mut self,
        edit: &brush_core::completion::Edit,
    ) -> Result<(), ShellError> {
        let replace = &edit.replace;
        let mut delete_count = self.cursor.saturating_sub(replace.start);
        let mut redisplay_offset = replace.start;

        // Don't bother erasing and re-writing the portion of the
        // completion's prefix that
        // is identical to what we already had in the token-being-completed.
        if delete_count > 0
            && self
                .line
                .get(redisplay_offset..self.cursor)
                .is_some_and(|typed| edit.text.starts_with(typed))
        {
            redisplay_offset = self.cursor;
            delete_count = 0;
        }

        if !self.apply_edit(edit) {
            return Ok(());
        }

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
        offers: &[crate::completion::Offer],
    ) -> Result<(), ShellError> {
        // Display replacements.
        Self::display_newline()?;
        for offer in offers {
            eprintln!("{}", offer.display);
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

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
