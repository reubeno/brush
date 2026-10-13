//! Drawing the prompt and the line being edited.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::Write;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::style::Style;

/// Draws a prompt and a line of input, as the [`Style`] has it drawn.
pub(crate) enum Renderer {
    Vt(VtRenderer),
    Backspace(BackspaceRenderer),
}

impl Renderer {
    pub fn new(style: Style) -> Self {
        match style {
            Style::Vt => Self::Vt(VtRenderer::default()),
            Style::Basic => Self::Backspace(BackspaceRenderer::default()),
        }
    }

    /// Draws `prompt` followed by `text`, and puts the cursor at byte offset `cursor`
    /// of `text`. Replaces what was drawn last time, if anything.
    ///
    /// # Arguments
    ///
    /// * `out` - The terminal.
    /// * `prompt` - The prompt, on a single line; may contain escape sequences, with
    ///   any other non-printing text between `\x01` and `\x02` markers.
    /// * `text` - The line being edited.
    /// * `cursor` - The cursor's byte offset in `text`.
    /// * `columns` - The terminal's width.
    pub fn draw(
        &mut self,
        out: &mut impl Write,
        prompt: &str,
        text: &str,
        cursor: usize,
        columns: usize,
    ) -> std::io::Result<()> {
        let (text, cursor) = shown_text(text, cursor);
        match self {
            Self::Vt(r) => r.draw(out, prompt, &text, cursor, columns),
            Self::Backspace(r) => r.draw(out, prompt, &text, cursor),
        }
    }

    /// Redraws the line with the cursor at its end, then moves to the start of the
    /// next row, leaving the line in place for whatever comes next.
    pub fn finish(
        &mut self,
        out: &mut impl Write,
        prompt: &str,
        text: &str,
        columns: usize,
    ) -> std::io::Result<()> {
        let (text, _) = shown_text(text, text.len());
        match self {
            Self::Vt(r) => r.finish(out, prompt, &text, columns),
            Self::Backspace(r) => r.finish(out, prompt, &text),
        }
    }

    /// Moves to the start of a row of its own to start drawing, as zsh does, so the
    /// line doesn't overwrite output that didn't end with a newline. Without cursor
    /// addressing, the line is drawn on from wherever the cursor is.
    ///
    /// # Arguments
    ///
    /// * `out` - The terminal.
    /// * `columns` - The terminal's width.
    pub fn start_on_own_row(&self, out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        match self {
            Self::Vt(_) => VtRenderer::start_on_own_row(out, columns),
            Self::Backspace(_) => Ok(()),
        }
    }

    /// Forgets what was drawn; the next draw starts afresh where the cursor is.
    pub fn reset(&mut self) {
        match self {
            Self::Vt(r) => r.reset(),
            Self::Backspace(r) => r.reset(),
        }
    }
}

/// Draws with VT100-style escape sequences.
///
/// Each change redraws the whole line, from the start of the prompt. That keeps
/// the logic simple and correct when the line wraps across rows, at a cost that's
/// negligible for lines people type.
#[derive(Default)]
pub(crate) struct VtRenderer {
    /// How many rows the cursor is below the row the prompt starts on.
    cursor_row: usize,
}

impl VtRenderer {
    /// Draws `prompt` followed by `text`, and puts the cursor at byte offset `cursor`
    /// of `text`. Replaces what was drawn last time, if anything.
    ///
    /// # Arguments
    ///
    /// * `out` - The terminal.
    /// * `prompt` - The prompt, on a single line; may contain escape sequences, with
    ///   any other non-printing text between `\x01` and `\x02` markers.
    /// * `text` - The line being edited.
    /// * `cursor` - The cursor's byte offset in `text`.
    /// * `columns` - The terminal's width.
    pub fn draw(
        &mut self,
        out: &mut impl Write,
        prompt: &str,
        text: &str,
        cursor: usize,
        columns: usize,
    ) -> std::io::Result<()> {
        let columns = columns.max(1);
        let mut seq = String::new();

        // Go back to where the prompt starts, and clear from there on.
        move_up(&mut seq, self.cursor_row);
        seq.push_str("\r\x1b[J");

        seq.push_str(&printable(prompt));
        seq.push_str(text);

        let shown_prompt = visible_text(prompt);
        let (end_row, end_column) =
            position_after(shown_prompt.chars().chain(text.chars()), columns);
        if end_column == 0 && end_row > 0 {
            // The line ends at the right margin, where terminals leave the cursor until
            // something else is written; put it at the start of the next row, where
            // `position_after` has it.
            seq.push('\n');
        }

        let before_cursor = text.get(..cursor).unwrap_or_default();
        let (mut target_row, mut target_column) =
            position_after(shown_prompt.chars().chain(before_cursor.chars()), columns);
        // The char at the cursor may not fit on what's left of the row, in which case
        // it's drawn at the start of the next; put the cursor on it there.
        if let Some(c) = text.get(cursor..).and_then(|rest| rest.chars().next())
            && !fits(target_column, char_width(c), columns)
        {
            (target_row, target_column) = (target_row + 1, 0);
        }

        move_up(&mut seq, end_row.saturating_sub(target_row));
        seq.push('\r');
        if target_column > 0 {
            let _ = write!(seq, "\x1b[{target_column}C");
        }

        self.cursor_row = target_row;
        out.write_all(seq.as_bytes())?;
        out.flush()
    }

    /// Redraws the line with the cursor at its end, then moves to the start of the
    /// next row, leaving the line in place for whatever comes next.
    pub fn finish(
        &mut self,
        out: &mut impl Write,
        prompt: &str,
        text: &str,
        columns: usize,
    ) -> std::io::Result<()> {
        self.draw(out, prompt, text, text.len(), columns)?;

        // If the line ended at the right margin, the cursor is already on a new row.
        let shown_prompt = visible_text(prompt);
        let (row, column) =
            position_after(shown_prompt.chars().chain(text.chars()), columns.max(1));
        if column > 0 || row == 0 {
            out.write_all(b"\n")?;
        }

        self.cursor_row = 0;
        out.flush()
    }

    /// Erases what was drawn, leaving the cursor where the prompt started, for the
    /// next draw to start from.
    pub fn erase(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        let mut seq = String::new();
        move_up(&mut seq, self.cursor_row);
        seq.push_str("\r\x1b[J");

        self.cursor_row = 0;
        out.write_all(seq.as_bytes())?;
        out.flush()
    }

    /// Moves to the start of the next row, unless the cursor is at the start of one.
    /// A row's worth of spaces from its first column leaves the cursor at the right
    /// margin of that row, waiting to wrap, so a carriage return goes back to its
    /// start; from any later column, they wrap onto the next row, leaving what was
    /// before them in place.
    pub fn start_on_own_row(out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        let mut seq = " ".repeat(columns.max(1));
        seq.push('\r');
        out.write_all(seq.as_bytes())
    }

    /// Forgets what was drawn; the next draw starts at the cursor's row.
    pub const fn reset(&mut self) {
        self.cursor_row = 0;
    }
}

/// Draws with backspaces and plain text alone, on a single row.
#[derive(Default)]
pub(crate) struct BackspaceRenderer {
    /// What was drawn last, if anything since the last reset.
    shown: Option<Shown>,
}

/// A line as drawn.
struct Shown {
    prompt: String,
    text: String,
    /// The cursor's byte offset in `text`.
    cursor: usize,
}

#[allow(
    clippy::string_slice,
    reason = "offsets are at char boundaries of the text they index"
)]
impl BackspaceRenderer {
    fn draw(
        &mut self,
        out: &mut impl Write,
        prompt: &str,
        text: &str,
        cursor: usize,
    ) -> std::io::Result<()> {
        let mut seq = String::new();

        // Only the cursor moved: move it there, over the text.
        if let Some(shown) = &self.shown
            && shown.prompt == prompt
            && shown.text == text
        {
            if cursor <= shown.cursor {
                push_backspaces(&mut seq, cells(&text[cursor..shown.cursor]));
            } else {
                seq.push_str(&text[shown.cursor..cursor]);
            }
            self.shown = Some(Shown {
                prompt: prompt.to_owned(),
                text: text.to_owned(),
                cursor,
            });
            out.write_all(seq.as_bytes())?;
            return out.flush();
        }

        // Where to rewrite from, as a byte offset in `text`, and how many cells
        // of the old line from there on are to be overwritten.
        let (from, old_cells) = match &self.shown {
            // Same prompt: rewrite from where the line changed, moving there from
            // the cursor.
            Some(shown) if shown.prompt == prompt => {
                let from = unchanged_prefix(&shown.text, text);
                let (shown_cursor, from_cells) =
                    (cells(&shown.text[..shown.cursor]), cells(&text[..from]));
                if shown_cursor >= from_cells {
                    push_backspaces(&mut seq, shown_cursor - from_cells);
                } else {
                    seq.push_str(&text[shown.cursor..from]);
                }
                (from, cells(&shown.text[from..]))
            }
            // A different prompt (e.g., searching history): rewrite the whole row.
            Some(shown) => {
                seq.push('\r');
                seq.push_str(&printable(prompt));
                let old = display_width(&shown.prompt) + cells(&shown.text);
                (0, old.saturating_sub(display_width(prompt)))
            }
            None => {
                seq.push_str(&printable(prompt));
                (0, 0)
            }
        };

        // Rewrite the rest of the line, blank whatever's left of the old one, and
        // move back to the cursor.
        let rewritten = &text[from..];
        let blanks = old_cells.saturating_sub(cells(rewritten));
        seq.push_str(rewritten);
        seq.push_str(&" ".repeat(blanks));
        push_backspaces(&mut seq, cells(&text[cursor..]) + blanks);

        self.shown = Some(Shown {
            prompt: prompt.to_owned(),
            text: text.to_owned(),
            cursor,
        });
        out.write_all(seq.as_bytes())?;
        out.flush()
    }

    /// Leaves the line as drawn (bringing it up to date if need be) and moves to
    /// the next row from wherever the cursor is, as readline does on a terminal
    /// like this.
    fn finish(&mut self, out: &mut impl Write, prompt: &str, text: &str) -> std::io::Result<()> {
        if self
            .shown
            .as_ref()
            .is_none_or(|shown| shown.prompt != prompt || shown.text != text)
        {
            self.draw(out, prompt, text, text.len())?;
        }
        self.shown = None;
        out.write_all(b"\n")?;
        out.flush()
    }

    fn reset(&mut self) {
        self.shown = None;
    }
}

/// Returns how long a prefix `old` and `new` share, in bytes, short of any
/// character that a zero-width character after it may combine with.
fn unchanged_prefix(old: &str, new: &str) -> usize {
    let mut len = old
        .char_indices()
        .zip(new.chars())
        .take_while(|((_, a), b)| a == b)
        .last()
        .map_or(0, |((i, c), _)| i + c.len_utf8());

    let combines_at = |s: &str, at: usize| {
        s.get(at..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.width() == Some(0))
    };
    while len > 0 && (combines_at(old, len) || combines_at(new, len)) {
        // Back up to the start of the char before.
        len = old
            .get(..len)
            .and_then(|s| s.char_indices().next_back())
            .map_or(0, |(i, _)| i);
    }
    len
}

/// Returns how many cells `text` takes on the terminal.
fn cells(text: &str) -> usize {
    text.width()
}

fn push_backspaces(seq: &mut String, count: usize) {
    seq.extend(std::iter::repeat_n('\x08', count));
}

/// Returns where the cursor is, as a row and column, after writing `chars` from
/// the start of a row on a terminal `columns` wide. As on a terminal, a char that
/// doesn't fit on what's left of a row (e.g., a wide char in the last column)
/// starts the next row, and a row once full leaves the cursor at the start of the
/// next.
fn position_after(chars: impl Iterator<Item = char>, columns: usize) -> (usize, usize) {
    let (mut row, mut column) = (0, 0);
    for width in chars.map(char_width).filter(|&width| width > 0) {
        if !fits(column, width, columns) {
            (row, column) = (row + 1, 0);
        }
        column += width;
        if column >= columns {
            (row, column) = (row + 1, 0);
        }
    }
    (row, column)
}

/// Whether a char `width` cells wide fits at `column` of a row `columns` wide. A
/// char too wide for any row is still put at a row's start.
const fn fits(column: usize, width: usize, columns: usize) -> bool {
    column == 0 || column + width <= columns
}

/// Returns how many cells `c` takes on the terminal.
fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

fn move_up(seq: &mut String, rows: usize) {
    if rows > 0 {
        let _ = write!(seq, "\x1b[{rows}A");
    }
}

/// Returns `text` as it's shown: with control chars in caret notation (e.g., `^J`
/// for a newline), as readline shows them, so that they take the cells they're
/// measured to and don't act on the terminal. Also returns `cursor`, a byte
/// offset in `text`, as one in what's returned.
fn shown_text(text: &str, cursor: usize) -> (Cow<'_, str>, usize) {
    if !text.chars().any(char::is_control) {
        return (Cow::Borrowed(text), cursor);
    }

    let mut shown = String::with_capacity(text.len() + 8);
    let mut shown_cursor = None;
    for (index, c) in text.char_indices() {
        if index == cursor {
            shown_cursor = Some(shown.len());
        }
        match u8::try_from(c) {
            Ok(byte) if c.is_ascii_control() => {
                shown.push('^');
                shown.push(char::from(byte ^ 0x40));
            }
            _ if c.is_control() => shown.push(char::REPLACEMENT_CHARACTER),
            _ => shown.push(c),
        }
    }
    let shown_cursor = shown_cursor.unwrap_or(shown.len());
    (Cow::Owned(shown), shown_cursor)
}

/// Returns `prompt` as it should be written: without the markers bracketing
/// non-printing text.
pub(crate) fn printable(prompt: &str) -> String {
    prompt.replace(['\x01', '\x02'], "")
}

/// Returns how many columns `s` takes on the terminal, leaving out escape
/// sequences and text marked as non-printing.
pub(crate) fn display_width(s: &str) -> usize {
    visible_text(s).width()
}

/// Returns the text of `s` that the terminal shows: without escape sequences,
/// text marked as non-printing, or other control chars.
fn visible_text(s: &str) -> String {
    let mut visible = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Explicitly non-printing text, as marked in prompts with `\[` and `\]`.
            '\x01' => {
                for c in chars.by_ref() {
                    if c == '\x02' {
                        break;
                    }
                }
            }
            '\x1b' => match chars.next() {
                // A control sequence: parameters and intermediates, then a final byte.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                // An operating system command (e.g., a window title), ended by BEL or
                // ST (`ESC \`).
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            c if c.is_control() => {}
            c => visible.push(c),
        }
    }
    visible
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in tests")]
mod tests {
    use super::*;

    /// Returns a terminal `columns` wide to play what's drawn on.
    fn terminal(columns: usize) -> vt100::Parser {
        vt100::Parser::new(24, u16::try_from(columns).unwrap(), 0)
    }

    /// Plays `output` on `terminal`, translating newlines to CR LF, as terminals do
    /// in raw mode.
    fn play(terminal: &mut vt100::Parser, output: &[u8]) {
        let output = std::str::from_utf8(output).unwrap().replace('\n', "\r\n");
        terminal.process(output.as_bytes());
    }

    /// Returns the rows `terminal` shows, up to the last that isn't blank, and the
    /// cursor's row and column.
    fn shown(terminal: &vt100::Parser) -> (Vec<String>, (usize, usize)) {
        let screen = terminal.screen();
        let mut rows: Vec<String> = screen
            .rows(0, screen.size().1)
            .map(|row| row.trim_end().to_owned())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        let (row, column) = screen.cursor_position();
        (rows, (usize::from(row), usize::from(column)))
    }

    /// Starts lines below output that didn't end with a newline, and where output
    /// that did end with one left the cursor.
    #[test]
    fn starts_lines_on_their_own_rows() -> std::io::Result<()> {
        const COLUMNS: usize = 8;
        for (output, expected) in [
            ("", vec!["$ ab"]),
            ("done\n", vec!["done", "$ ab"]),
            ("h", vec!["h", "$ ab"]),
            ("hello", vec!["hello", "$ ab"]),
            ("1234567", vec!["1234567", "$ ab"]),
            ("12345678", vec!["12345678", "$ ab"]),
        ] {
            let mut terminal = terminal(COLUMNS);
            play(&mut terminal, output.replace('\n', "\r\n").as_bytes());

            let mut out = vec![];
            VtRenderer::start_on_own_row(&mut out, COLUMNS)?;
            VtRenderer::default().draw(&mut out, "$ ", "ab", 2, COLUMNS)?;
            play(&mut terminal, &out);

            let (rows, _) = shown(&terminal);
            assert_eq!(rows, expected, "after {output:?}");
        }
        Ok(())
    }

    /// Draws `text` after a prompt on a narrow screen, as a sequence of edits, and
    /// checks the screen and cursor after each.
    #[test]
    fn redraws_in_place_across_wrapped_rows() -> std::io::Result<()> {
        const COLUMNS: usize = 5;
        let prompt = "\x01\x1b[1m\x02$\x1b[0m ";
        let mut terminal = terminal(COLUMNS);
        let mut renderer = VtRenderer::default();

        // Each step is the text, the cursor's position in it, and the cursor's
        // expected row and column.
        for (text, cursor, row, column) in [
            ("", 0, 0, 2),
            ("abc", 3, 1, 0),
            ("abcdefg", 7, 1, 4),
            ("abcdefgh", 8, 2, 0),
            ("abcdefgh", 1, 0, 3),
            ("abcd", 4, 1, 1),
            ("a", 0, 0, 2),
        ] {
            let mut out = vec![];
            renderer.draw(&mut out, prompt, text, cursor, COLUMNS)?;
            play(&mut terminal, &out);

            let (rows, position) = shown(&terminal);
            assert_eq!(
                rows.concat(),
                format!("$ {text}").trim_end(),
                "drawing {text:?}"
            );
            assert_eq!(position, (row, column), "drawing {text:?}");
        }

        // Finishing leaves the line and moves to a fresh row.
        let mut out = vec![];
        renderer.finish(&mut out, prompt, "abcdefgh", COLUMNS)?;
        play(&mut terminal, &out);
        let (rows, position) = shown(&terminal);
        assert_eq!(rows.concat(), "$ abcdefgh");
        assert_eq!(position, (2, 0));

        Ok(())
    }

    /// Returns the cell where a terminal that's `columns` wide draws the char after
    /// `before`, wrapping as it does if that char doesn't fit on the row. For a char
    /// that takes no cells (e.g., a combining accent), it's the cell where the next
    /// char that does would go.
    fn cell_after(before: &str, next: char, columns: usize) -> (usize, usize) {
        let probe = if next.width().unwrap_or(0) == 0 {
            'X'
        } else {
            next
        };
        let mut terminal = terminal(columns);
        play(&mut terminal, format!("{before}{probe}").as_bytes());

        // The probe is the last thing written, so it's in the last cell in use.
        let screen = terminal.screen();
        let (rows, cols) = screen.size();
        (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (row, col)))
            .rfind(|&(row, col)| {
                screen
                    .cell(row, col)
                    .is_some_and(|cell| cell.has_contents() && !cell.is_wide_continuation())
            })
            .map(|(row, col)| (usize::from(row), usize::from(col)))
            .unwrap()
    }

    /// Draws lines of wide, narrow, and combining chars on narrow terminals, with
    /// the cursor at each char in turn, and checks against a terminal emulator that
    /// the line shows in full and the cursor is on the char it's at: where the
    /// terminal draws that char, which for a wide char that doesn't fit on what's
    /// left of a row is the start of the next.
    #[test]
    fn places_the_cursor_on_wide_and_combining_chars_across_rows() -> std::io::Result<()> {
        for prompt in ["$ ", "界> ", "\x01\x1b[1m\x02$\x1b[0m "] {
            for text in [
                "abc界d",
                "界界界界",
                "a界b界c",
                "e\u{301}界x",
                "日本語のテキスト",
            ] {
                for columns in 3..=9 {
                    let mut renderer = VtRenderer::default();
                    let mut terminal = terminal(columns);
                    let shown_prompt = visible_text(prompt);
                    let case = format!("{prompt:?} {text:?} in {columns} columns");
                    // The rows lose the spaces at their ends, including the prompt's
                    // when what follows it wraps; the texts have none of their own.
                    let expected = format!("{shown_prompt}{text}").replace(' ', "");
                    let without_spaces = |rows: &[String]| rows.concat().replace(' ', "");

                    let boundaries = text.char_indices().map(|(i, _)| i).chain([text.len()]);
                    for cursor in boundaries {
                        let mut out = vec![];
                        renderer.draw(&mut out, prompt, text, cursor, columns)?;
                        play(&mut terminal, &out);

                        let (rows, position) = shown(&terminal);
                        assert_eq!(without_spaces(&rows), expected, "{case}");
                        let (head, tail) = text.split_at(cursor);
                        let before = format!("{shown_prompt}{head}");
                        let next = tail.chars().next().unwrap_or('X');
                        assert_eq!(
                            position,
                            cell_after(&before, next, columns),
                            "{case}, cursor at {cursor}"
                        );
                    }

                    // Finishing leaves the line in place, and the cursor at the start
                    // of the row after it.
                    let mut out = vec![];
                    renderer.finish(&mut out, prompt, text, columns)?;
                    play(&mut terminal, &out);
                    let (rows, position) = shown(&terminal);
                    assert_eq!(without_spaces(&rows), expected, "{case}");
                    assert_eq!(position, (rows.len(), 0), "{case}");
                }
            }
        }
        Ok(())
    }

    /// Draws lines in turn with backspaces alone, and checks that the terminal
    /// shows each, with the cursor in place, however it differs from the last:
    /// shorter, longer, changed mid-line, with chars that take two cells or none,
    /// or with a different prompt.
    #[test]
    fn redraws_with_backspaces() -> std::io::Result<()> {
        let mut renderer = BackspaceRenderer::default();
        let mut terminal = terminal(80);

        // Each step is the prompt, the text, and the cursor's position in it.
        for (prompt, text, cursor) in [
            ("$ ", "", 0),
            ("$ ", "cmd abcdef", 10),
            ("$ ", "cmd b", 5),
            ("$ ", "cmd b 界", 5),
            ("$ ", "cmd xyz 界", 7),
            ("$ ", "cmd xyz 界", 0),
            ("$ ", "cmd e 界", 5),
            ("$ ", "cmd e\u{301} 界", 7),
            ("$ ", "cmd x 界", 5),
            ("$ ", "cmd 界", 4),
            ("$ ", "cmd 界x", 8),
            ("(reverse-i-search)`x': ", "cmd 界x", 7),
            ("$ ", "cmd", 3),
        ] {
            let mut out = vec![];
            renderer.draw(&mut out, prompt, text, cursor)?;
            play(&mut terminal, &out);

            let (rows, position) = shown(&terminal);
            assert_eq!(
                rows.concat(),
                format!("{prompt}{text}").trim_end(),
                "drawing {text:?}"
            );
            let column = cells(prompt) + text.get(..cursor).map_or(0, cells);
            assert_eq!(
                position,
                (0, column),
                "drawing {text:?} with cursor at {cursor}"
            );
        }

        Ok(())
    }

    /// Moving the cursor, and finishing the line, write no more than they must:
    /// what test harnesses expect, having been written for readline.
    #[test]
    fn moves_with_backspaces_and_finishes_in_place() -> std::io::Result<()> {
        let mut renderer = BackspaceRenderer::default();
        let drawn = |renderer: &mut BackspaceRenderer, text: &str, cursor: usize| {
            let mut out = vec![];
            renderer.draw(&mut out, "$ ", text, cursor).map(|()| out)
        };

        assert_eq!(drawn(&mut renderer, "alias foo", 9)?, b"$ alias foo");
        assert_eq!(drawn(&mut renderer, "alias foo", 8)?, b"\x08");
        assert_eq!(drawn(&mut renderer, "alias foo", 6)?, b"\x08\x08");
        assert_eq!(drawn(&mut renderer, "alias foo", 7)?, b"f");

        let mut out = vec![];
        renderer.finish(&mut out, "$ ", "alias foo")?;
        assert_eq!(out, b"\n");

        Ok(())
    }

    #[test]
    fn measures_only_visible_text() {
        assert_eq!(display_width("\x01\x1b[1;32m\x02~/x\x01\x1b[0m\x02 $ "), 6);
        assert_eq!(display_width("\x1b]0;title\x07> "), 2);
        assert_eq!(display_width("\x1b]0;title\x1b\\> "), 2);
        assert_eq!(display_width("界> "), 4);
        // Other escapes take the char after them; other controls take no room.
        assert_eq!(display_width("\x1b7\t> "), 2);
    }

    /// Draws a sequence of lines with `renderer`, then finishes the last, and
    /// returns what the terminal shows.
    fn draw_all(
        renderer: &mut Renderer,
        lines: &[(&str, usize)],
        finish: &str,
    ) -> std::io::Result<(Vec<String>, (usize, usize))> {
        let mut out = vec![];
        for (text, cursor) in lines {
            renderer.draw(&mut out, "$ ", text, *cursor, 80)?;
        }
        renderer.finish(&mut out, "$ ", finish, 80)?;
        let mut terminal = terminal(80);
        play(&mut terminal, &out);
        Ok(shown(&terminal))
    }

    #[test]
    fn draws_in_either_style() -> std::io::Result<()> {
        for style in [Style::Vt, Style::Basic] {
            let mut renderer = Renderer::new(style);
            let (rows, position) = draw_all(&mut renderer, &[("ab", 2), ("abc", 1)], "abcd")?;
            assert_eq!(rows, ["$ abcd"], "{style:?}");
            assert_eq!(position, (1, 0), "{style:?}");
        }
        Ok(())
    }

    #[test]
    fn starts_afresh_after_a_reset() -> std::io::Result<()> {
        for style in [Style::Vt, Style::Basic] {
            let mut renderer = Renderer::new(style);
            let mut out = vec![];
            renderer.draw(&mut out, "$ ", "ab", 2, 80)?;
            out.extend_from_slice(b"\n");
            renderer.reset();
            renderer.finish(&mut out, "$ ", "cd", 80)?;

            let mut terminal = terminal(80);
            play(&mut terminal, &out);
            assert_eq!(shown(&terminal).0, ["$ ab", "$ cd"], "{style:?}");
        }
        Ok(())
    }

    #[test]
    fn finishes_with_backspaces_by_drawing_what_changed() -> std::io::Result<()> {
        let mut renderer = BackspaceRenderer::default();
        let mut out = vec![];
        renderer.draw(&mut out, "$ ", "ab", 1)?;
        out.clear();
        renderer.finish(&mut out, "$ ", "abc")?;
        assert_eq!(out, b"bc\n");

        // With nothing drawn yet, it draws the whole line.
        out.clear();
        renderer.finish(&mut out, "$ ", "x")?;
        assert_eq!(out, b"$ x\n");
        Ok(())
    }

    #[test]
    fn shows_control_characters_in_caret_notation() -> std::io::Result<()> {
        assert_eq!(shown_text("plain", 2), (Cow::Borrowed("plain"), 2));
        assert_eq!(
            shown_text("a\nb\t\x1b[m\x7f\u{85}é", 2),
            (Cow::Owned("a^Jb^I^[[m^?\u{FFFD}é".to_owned()), 3)
        );

        // Both styles draw them so, with the cursor after them where it should be.
        for style in [Style::Vt, Style::Basic] {
            let mut renderer = Renderer::new(style);
            let mut out = vec![];
            renderer.draw(&mut out, "$ ", "ab\ncd", 4, 80)?;
            let mut terminal = terminal(80);
            play(&mut terminal, &out);
            assert_eq!(
                shown(&terminal),
                (vec!["$ ab^Jcd".to_owned()], (0, 7)),
                "{style:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn treats_a_zero_width_terminal_as_one_column_wide() -> std::io::Result<()> {
        let mut renderer = VtRenderer::default();
        let mut out = vec![];
        renderer.draw(&mut out, "", "ab", 1, 0)?;
        let mut terminal = terminal(1);
        play(&mut terminal, &out);
        assert_eq!(
            shown(&terminal),
            (vec!["a".to_owned(), "b".to_owned()], (1, 0))
        );
        Ok(())
    }
}
