//! The line being edited, and the edits that can be made to it.

#![allow(
    clippy::string_slice,
    reason = "the cursor and the offsets derived from it are kept at char boundaries"
)]

use std::ops::Range;

/// A line of text being edited, with a cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineBuffer {
    text: String,
    /// The cursor's byte offset in `text`; always at a character boundary.
    cursor: usize,
}

impl LineBuffer {
    /// Returns a buffer holding `text`, with the cursor at its end.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the buffer's text, putting the cursor at `cursor` (clamped to a
    /// character boundary within the text).
    pub fn set(&mut self, text: String, cursor: usize) {
        self.text = text;
        self.move_to(cursor);
    }

    /// Moves the cursor to byte offset `cursor` (clamped to a character boundary
    /// within the text).
    pub fn move_to(&mut self, cursor: usize) {
        let mut cursor = cursor.min(self.text.len());
        while !self.text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        self.cursor = cursor;
    }

    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Replaces the text in `range` with `replacement`, leaving the cursor just after
    /// it.
    pub fn replace(&mut self, range: Range<usize>, replacement: &str) {
        self.text.replace_range(range.clone(), replacement);
        self.cursor = range.start + replacement.len();
    }

    pub fn backspace(&mut self) {
        let start = self.prev_char_boundary(self.cursor);
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn delete(&mut self) {
        let end = self.next_char_boundary(self.cursor);
        self.text.replace_range(self.cursor..end, "");
    }

    pub fn left(&mut self) {
        self.cursor = self.prev_char_boundary(self.cursor);
    }

    pub fn right(&mut self) {
        self.cursor = self.next_char_boundary(self.cursor);
    }

    pub const fn home(&mut self) {
        self.cursor = 0;
    }

    pub const fn end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn word_left(&mut self) {
        self.cursor = self.word_start_before(is_word_char);
    }

    pub fn word_right(&mut self) {
        self.cursor = self.word_end_after(is_word_char);
    }

    /// Deletes from the start of the line to the cursor, returning what was deleted.
    pub fn kill_to_start(&mut self) -> String {
        self.kill(0..self.cursor)
    }

    /// Deletes from the cursor to the end of the line, returning what was deleted.
    pub fn kill_to_end(&mut self) -> String {
        self.kill(self.cursor..self.text.len())
    }

    /// Deletes the whitespace-delimited word before the cursor (like readline's
    /// `unix-word-rubout`), returning what was deleted.
    pub fn kill_big_word_before(&mut self) -> String {
        let start = self.word_start_before(|c| !c.is_whitespace());
        self.kill(start..self.cursor)
    }

    /// Deletes the word before the cursor, returning what was deleted.
    pub fn kill_word_before(&mut self) -> String {
        let start = self.word_start_before(is_word_char);
        self.kill(start..self.cursor)
    }

    /// Deletes the word after the cursor, returning what was deleted.
    pub fn kill_word_after(&mut self) -> String {
        let end = self.word_end_after(is_word_char);
        self.kill(self.cursor..end)
    }

    /// Swaps the characters before and at the cursor (or the two before it, at the
    /// end of the line), and moves the cursor past them. Does nothing unless there
    /// are two characters to swap.
    pub fn transpose(&mut self) {
        let b = if self.cursor == self.text.len() {
            self.prev_char_boundary(self.cursor)
        } else {
            self.cursor
        };
        let a = self.prev_char_boundary(b);
        let c = self.next_char_boundary(b);
        if a < b && b < c {
            let swapped = std::format!("{}{}", &self.text[b..c], &self.text[a..b]);
            self.text.replace_range(a..c, &swapped);
            self.cursor = c;
        }
    }

    fn kill(&mut self, range: Range<usize>) -> String {
        let killed = self.text[range.clone()].to_owned();
        self.text.replace_range(range.clone(), "");
        self.cursor = range.start;
        killed
    }

    /// Returns the start of the char before byte offset `pos`, or 0 at the start.
    fn prev_char_boundary(&self, pos: usize) -> usize {
        self.text[..pos]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    /// Returns the end of the char at byte offset `pos`, or `pos` at the end.
    fn next_char_boundary(&self, pos: usize) -> usize {
        self.text[pos..]
            .chars()
            .next()
            .map_or(pos, |c| pos + c.len_utf8())
    }

    /// Finds the start of the word before the cursor: skips back over non-word
    /// chars, then over word chars.
    fn word_start_before(&self, in_word: impl Fn(char) -> bool) -> usize {
        self.text[..self.cursor]
            .trim_end_matches(|c| !in_word(c))
            .trim_end_matches(&in_word)
            .len()
    }

    /// Finds the end of the word after the cursor: skips over non-word chars, then
    /// over word chars.
    fn word_end_after(&self, in_word: impl Fn(char) -> bool) -> usize {
        let rest = self.text[self.cursor..]
            .trim_start_matches(|c| !in_word(c))
            .trim_start_matches(&in_word);
        self.text.len() - rest.len()
    }
}

/// Whether `c` is part of a word, for word-wise movement and deletion.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a buffer for `marked`, with the cursor where `|` is.
    fn buffer(marked: &str) -> LineBuffer {
        let cursor = marked.find('|').unwrap_or_default();
        let mut buffer = LineBuffer::default();
        buffer.set(marked.replacen('|', "", 1), cursor);
        buffer
    }

    /// Shows a buffer with `|` at the cursor.
    fn marked(buffer: &LineBuffer) -> String {
        let mut s = buffer.text().to_owned();
        s.insert(buffer.cursor(), '|');
        s
    }

    #[test]
    fn moves_by_characters() {
        let mut b = buffer("aé|界b");
        b.left();
        assert_eq!(marked(&b), "a|é界b");
        b.right();
        b.right();
        assert_eq!(marked(&b), "aé界|b");
        b.backspace();
        assert_eq!(marked(&b), "aé|b");
        b.delete();
        assert_eq!(marked(&b), "aé|");
        b.delete();
        assert_eq!(marked(&b), "aé|");
    }

    #[test]
    fn moves_by_words() {
        let mut b = buffer("echo foo-bar  baz|");
        b.word_left();
        assert_eq!(marked(&b), "echo foo-bar  |baz");
        b.word_left();
        assert_eq!(marked(&b), "echo foo-|bar  baz");
        b.word_right();
        assert_eq!(marked(&b), "echo foo-bar|  baz");
        b.home();
        b.word_left();
        assert_eq!(marked(&b), "|echo foo-bar  baz");
    }

    #[test]
    fn kills_text() {
        let mut b = buffer("ls -l foo/bar|");
        assert_eq!(b.kill_word_before(), "bar");
        assert_eq!(marked(&b), "ls -l foo/|");
        assert_eq!(b.kill_big_word_before(), "foo/");
        assert_eq!(marked(&b), "ls -l |");
        b.home();
        assert_eq!(b.kill_word_after(), "ls");
        assert_eq!(marked(&b), "| -l ");
        assert_eq!(b.kill_to_end(), " -l ");
        assert_eq!(marked(&b), "|");

        let mut b = buffer("abc|def");
        assert_eq!(b.kill_to_start(), "abc");
        assert_eq!(marked(&b), "|def");
    }

    #[test]
    fn kills_whitespace_before_words() {
        let mut b = buffer("   |");
        assert_eq!(b.kill_big_word_before(), "   ");
        assert_eq!(marked(&b), "|");
    }

    #[test]
    fn transposes_only_when_there_are_two_characters() {
        for marked_text in ["a|", "|a", "|"] {
            let mut b = buffer(marked_text);
            b.transpose();
            assert_eq!(marked(&b), marked_text);
        }
        let mut b = buffer("|ab");
        b.transpose();
        assert_eq!(marked(&b), "|ab");
    }

    #[test]
    fn transposes_characters() {
        let mut b = buffer("ab|c");
        b.transpose();
        assert_eq!(marked(&b), "acb|");
        b.transpose();
        assert_eq!(marked(&b), "abc|");
    }

    #[test]
    fn replaces_ranges() {
        let mut b = buffer("cat RE|");
        b.replace(4..6, "README.txt ");
        assert_eq!(marked(&b), "cat README.txt |");
    }

    #[test]
    fn keeps_the_cursor_on_a_character_boundary() {
        let mut b = LineBuffer::default();
        // Offset 2 is inside `é`, and 9 is past the end.
        b.set("aé".to_owned(), 2);
        assert_eq!(marked(&b), "a|é");
        b.set("aé".to_owned(), 9);
        assert_eq!(marked(&b), "aé|");
    }

    #[test]
    fn moves_and_kills_by_words_of_any_script() {
        let mut b = buffer("echo café naïve 日本語|");
        b.word_left();
        assert_eq!(marked(&b), "echo café naïve |日本語");
        b.word_left();
        assert_eq!(marked(&b), "echo café |naïve 日本語");
        b.word_right();
        assert_eq!(marked(&b), "echo café naïve| 日本語");
        assert_eq!(b.kill_word_before(), "naïve");
        assert_eq!(marked(&b), "echo café | 日本語");
        assert_eq!(b.kill_word_after(), " 日本語");
        assert_eq!(b.kill_big_word_before(), "café ");
        assert_eq!(marked(&b), "echo |");
    }

    #[test]
    fn transposes_multibyte_characters() {
        let mut b = buffer("é|ñ");
        b.transpose();
        assert_eq!(marked(&b), "ñé|");
        b.transpose();
        assert_eq!(marked(&b), "éñ|");
        let mut b = buffer("a界|");
        b.transpose();
        assert_eq!(marked(&b), "界a|");
    }

    #[test]
    fn edits_a_character_at_a_time_even_when_characters_combine() {
        // Like readline, the cursor moves over, and backspace deletes, one
        // character: an accent that combines with the letter before it is one too.
        let mut b = buffer("e\u{301}|x");
        b.backspace();
        assert_eq!(marked(&b), "e|x");
        b.insert("\u{301}");
        b.left();
        assert_eq!(marked(&b), "e|\u{301}x");
        b.delete();
        assert_eq!(marked(&b), "e|x");
    }

    #[test]
    fn kills_and_replaces_around_multibyte_characters() {
        let mut b = buffer("日本|語");
        assert_eq!(b.kill_to_end(), "語");
        assert_eq!(b.kill_to_start(), "日本");
        b.insert("café");
        // `é` is the two bytes at 3..5.
        b.replace(3..5, "e\u{301}");
        assert_eq!(marked(&b), "cafe\u{301}|");
    }
}
