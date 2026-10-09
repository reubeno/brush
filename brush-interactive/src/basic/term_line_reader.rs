//
// This module is intentionally limited, and does not have all the bells and whistles. We wan
// enough here that we can use it in the basic shell for (p)expect/pty-style testing of
// completion, and without using VT100-style escape sequences for cursor movement and display.
//

use crossterm::ExecutableCommand;
use std::io::Write;
use unicode_width::UnicodeWidthStr;

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
        mut completion_handler: impl FnMut(
            &str,
            usize,
        )
            -> Result<brush_core::completion::Completions, ShellError>,
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

    fn handle_completions(
        &mut self,
        completions: &brush_core::completion::Completions,
    ) -> Result<(), ShellError> {
        if completions.candidates.is_empty() {
            // Do nothing
            Ok(())
        } else if completions.candidates.len() == 1 {
            self.handle_single_completion(completions, &mut std::io::stderr())
        } else {
            self.handle_multiple_completions(completions)
        }
    }

    #[expect(
        clippy::string_slice,
        reason = "all offsets are expected to be at char boundaries"
    )]
    /// Completes the word with the only candidate in `completions`, and shows it on `out`,
    /// the terminal.
    fn handle_single_completion(
        &mut self,
        completions: &brush_core::completion::Completions,
        out: &mut impl Write,
    ) -> Result<(), ShellError> {
        let Some(candidate) = completions.candidates.first() else {
            return Ok(());
        };

        let replace =
            completions.insertion_index..completions.insertion_index + completions.delete_count;
        if replace.end != self.cursor {
            return Ok(());
        }

        // Don't rewrite what's already typed of the word, if the candidate keeps it.
        let redisplay_offset = if self
            .line
            .get(replace.clone())
            .is_some_and(|typed| candidate.starts_with(typed))
        {
            self.cursor
        } else {
            replace.start
        };

        // How much of the line is shown from there, and how far back that is from the
        // cursor, in terminal cells.
        let width = |text: Option<&str>| text.map_or(0, UnicodeWidthStr::width);
        let old_width = width(self.line.get(redisplay_offset..));
        let move_left = width(self.line.get(redisplay_offset..self.cursor));

        self.line.replace_range(replace.clone(), candidate);
        self.cursor = replace.start + candidate.len();

        // Rewrite the line from there, blanking whatever's left of it if it got shorter,
        // then move back to the cursor.
        let rewritten = &self.line[redisplay_offset..];
        let blanks = old_width.saturating_sub(rewritten.width());
        let move_back = self.line[self.cursor..].width() + blanks;
        write!(
            out,
            "{}{rewritten}{}{}",
            repeated_char_str(BACKSPACE, move_left),
            repeated_char_str(' ', blanks),
            repeated_char_str(BACKSPACE, move_back)
        )?;

        out.flush()?;

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
        let trimmed = brush_core::sys::fs::strip_path_separator_suffix(candidate);
        if let Some(index) = brush_core::sys::fs::rfind_path_separator(trimmed) {
            candidate = &candidate[index + 1..];
        }
    }

    candidate.to_string()
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use super::*;

    /// Plays `output` on a terminal line showing `shown`, with the cursor `cursor` cells in,
    /// as a terminal would: a backspace moves left a cell, and a char overwrites the cells it
    /// takes. Returns what the line then shows (without trailing spaces) and the cursor's
    /// cell.
    fn play(shown: &str, cursor: usize, output: &[u8]) -> (String, usize) {
        let mut screen = Vec::new();
        let mut pos = 0;
        for c in shown.chars() {
            put(&mut screen, &mut pos, c);
        }
        pos = cursor;
        for c in String::from_utf8_lossy(output).chars() {
            if c == BACKSPACE {
                pos = pos.saturating_sub(1);
            } else {
                put(&mut screen, &mut pos, c);
            }
        }
        (screen.concat().trim_end().to_owned(), pos)
    }

    /// Puts `c` on `screen`, a line's cells, at `pos`, moving `pos` past it, as a terminal
    /// would: a wide char takes two cells (the second left empty here), overwriting half of
    /// one blanks the other half, and a combining char joins the char before it.
    fn put(screen: &mut Vec<String>, pos: &mut usize, c: char) {
        use unicode_width::UnicodeWidthChar;

        let width = c.width().unwrap_or(0);
        if width == 0 {
            if let Some(cell) = pos.checked_sub(1).and_then(|p| screen.get_mut(p)) {
                cell.push(c);
            }
            return;
        }

        let cells = *pos..*pos + width;
        if screen.len() < cells.end {
            screen.resize(cells.end, " ".to_owned());
        }
        if cells.start > 0 && screen[cells.start].is_empty() {
            screen[cells.start - 1] = " ".to_owned();
        }
        if screen.get(cells.end).is_some_and(String::is_empty) {
            screen[cells.end] = " ".to_owned();
        }
        screen[cells.start] = c.to_string();
        if width == 2 {
            screen[cells.start + 1] = String::new();
        }
        *pos = cells.end;
    }

    /// The line shown after a completion is the line that will run, even when the candidate
    /// is shorter than the word or isn't a plain extension of it, or the line has chars that
    /// take two cells or none.
    #[test]
    fn completion_redraws_the_line_it_leaves() -> Result<(), ShellError> {
        for (line, replace, text, expected) in [
            ("cmd abcdef", 4..10, "b", "cmd b"),
            ("cmd ab", 4..6, "abc", "cmd abc"),
            ("cmd ab", 4..6, "xyz", "cmd xyz"),
            ("cmd é", 4..6, "e", "cmd e"),
            ("cmd ab", 4..6, "éé", "cmd éé"),
            ("cmd 界", 4..7, "e", "cmd e"),
            ("cmd x", 4..5, "界", "cmd 界"),
            ("cmd e\u{301}", 4..7, "x", "cmd x"),
            // The cursor is where the word ends, and the line goes on past it.
            ("cmd ab 界", 4..6, "x", "cmd x 界"),
        ] {
            use unicode_width::UnicodeWidthStr;
            let cells = |s: &str, end: usize| s.get(..end).map_or(0, UnicodeWidthStr::width);

            let mut state = ReadLineState::new(None);
            state.line = line.to_owned();
            state.cursor = replace.end;
            let completions = brush_core::completion::Completions {
                insertion_index: replace.start,
                delete_count: replace.len(),
                candidates: vec![text.to_owned()],
                ..Default::default()
            };

            let mut out = Vec::new();
            state.handle_single_completion(&completions, &mut out)?;

            let (shown, cursor) = play(line, cells(line, replace.end), &out);
            assert_eq!(shown, expected, "{line:?} -> {text:?}");
            assert_eq!(state.line, expected, "{line:?} -> {text:?}");
            let expected_cursor = cells(expected, replace.start + text.len());
            assert_eq!(cursor, expected_cursor, "{line:?} -> {text:?}");
        }

        Ok(())
    }
}
