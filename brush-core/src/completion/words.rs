//! Finds the words of an input line being completed, as bash does: both the word to
//! complete (readline's) and the words of `COMP_WORDS`.

use std::ops::Range;

use super::quoting::{self, WordQuoting};

/// The words of `COMP_WORDS` in an input line being completed, and which the cursor is in.
#[derive(Debug)]
pub(super) struct LineWords<'a> {
    /// The words of `COMP_WORDS`.
    pub comp_words: Vec<&'a str>,
    /// The index of the cursor's word in `comp_words` (`COMP_CWORD`), if there are any
    /// words.
    pub cword: Option<usize>,
}

/// No words, as in an empty line.
pub(super) static NO_WORDS: LineWords<'static> = LineWords {
    comp_words: Vec::new(),
    cword: None,
};

impl<'a> LineWords<'a> {
    /// Returns the command being completed for: the first word, if any.
    pub fn command_name(&self) -> Option<&'a str> {
        self.comp_words.first().copied()
    }

    /// Returns the word before the cursor's word, if any; like bash, when completing the
    /// first word, that word itself.
    pub fn preceding_word(&self) -> Option<&'a str> {
        let index = self.cword?.saturating_sub(1);
        self.comp_words.get(index).copied()
    }
}

/// The word being completed -- the text that completions replace -- and how it's quoted.
///
/// Like readline, it runs up to the cursor from just past an unclosed quote the cursor is
/// in, or else from just past the last unquoted word-break char (see `COMP_WORDBREAKS`),
/// so it may be only part of a shell word.
#[derive(Debug)]
pub(super) struct CompletionWord {
    /// The word's text, without its unclosed quote; a completion function's `$2`.
    pub text: String,
    /// The byte range of the line that candidates replace: the word's text, and the
    /// unclosed quote before it, if any.
    pub range: Range<usize>,
    /// How the word is quoted.
    pub quoting: WordQuoting,
}

/// A word of `COMP_WORDS`, and where it is in the input line.
#[derive(Debug, Clone, Copy)]
struct CompWord<'a> {
    /// The word's text.
    text: &'a str,
    /// The byte offset in the input line where the word starts.
    start: usize,
}

impl CompWord<'_> {
    /// Returns the byte offset in the input line just past the word.
    const fn end(&self) -> usize {
        self.start + self.text.len()
    }
}

/// Finds the words of `COMP_WORDS` in `input`, with `word_breaks` as the word-break chars,
/// and which of them the cursor is in. They needn't agree with the word to complete (see
/// [`find_completion_word`] and the module docs of [`completion`](super)).
pub(super) fn find_line_words<'a>(
    input: &'a str,
    word_breaks: &[char],
    cursor: usize,
) -> LineWords<'a> {
    let mut comp_words = comp_words(input, word_breaks);

    // Like bash, the cursor's word in COMP_WORDS is the last one touching the cursor
    // (so one starting there wins over one ending there). If none does, the cursor is
    // in whitespace, so insert an empty word there -- unless there are no words at all.
    let cword = (!comp_words.is_empty()).then(|| {
        comp_words
            .iter()
            .rposition(|w| w.start <= cursor && cursor <= w.end())
            .unwrap_or_else(|| {
                let index = comp_words.partition_point(|w| w.start < cursor);
                comp_words.insert(
                    index,
                    CompWord {
                        text: "",
                        start: cursor,
                    },
                );
                index
            })
    });

    LineWords {
        comp_words: comp_words.into_iter().map(|w| w.text).collect(),
        cword,
    }
}

/// Returns whether a scanned char breaks words: if it's an unquoted word-break char.
fn is_break(scanned: &quoting::ScannedChar, word_breaks: &[char]) -> bool {
    scanned.unquoted && word_breaks.contains(&scanned.c)
}

/// Finds the word to complete like readline: if the cursor is in an unclosed quote, it
/// starts just past that quote; otherwise, just past the last unquoted word-break char before the
/// cursor (or at it, if it's one of bash's special prefixes, `$` or `@`).
///
/// `cursor` must be a char boundary in `input`.
#[allow(
    clippy::string_slice,
    reason = "the word starts at a char boundary, and the caller checks the cursor is one"
)]
pub(super) fn find_completion_word(
    input: &str,
    word_breaks: &[char],
    cursor: usize,
) -> CompletionWord {
    const SPECIAL_PREFIXES: [char; 2] = ['$', '@'];

    let mut word_start = 0;
    let mut open_quote = None;
    for scanned in quoting::scan(input).take_while(|s| s.index < cursor) {
        if is_break(&scanned, word_breaks) {
            word_start = scanned.index;
            if !SPECIAL_PREFIXES.contains(&scanned.c) {
                word_start += scanned.c.len_utf8();
            }
        }
        open_quote = scanned.open_quote;
    }

    let (start, text_start, quote) = match open_quote {
        Some((index, q)) => (index, index + q.as_char().len_utf8(), Some(q)),
        None => (word_start, word_start, None),
    };

    CompletionWord {
        text: input[text_start..cursor].to_owned(),
        range: start..cursor,
        quoting: WordQuoting {
            quote,
            dequote: quoting::has_quoting(&input[..cursor]),
            dequote_again: false,
        },
    }
}

/// Splits `input` into the words of `COMP_WORDS`, like bash: runs of chars other than
/// word breaks are words, and so are runs of word-break chars other than whitespace;
/// whitespace separates words but isn't one.
#[allow(clippy::string_slice, reason = "used indices come from char_indices")]
fn comp_words<'a>(input: &'a str, word_breaks: &[char]) -> Vec<CompWord<'a>> {
    let mut tokens = vec![];
    let mut word_start = None;
    let mut word_is_breaks = false;

    for scanned in quoting::scan(input) {
        let (i, c) = (scanned.index, scanned.c);
        let is_break = is_break(&scanned, word_breaks);
        let is_whitespace = is_break && c.is_ascii_whitespace();

        // A word ends at whitespace, or where it switches between word breaks and other
        // chars.
        if let Some(start) = word_start {
            if is_whitespace || is_break != word_is_breaks {
                tokens.push(CompWord {
                    text: &input[start..i],
                    start,
                });
                word_start = None;
            }
        }

        if !is_whitespace && word_start.is_none() {
            word_start = Some(i);
            word_is_breaks = is_break;
        }
    }

    if let Some(start) = word_start {
        tokens.push(CompWord {
            text: &input[start..],
            start,
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::quoting::Quote;

    #[test]
    fn completion_word_like_readline() {
        let word_with = |delims: &str, input: &str, cursor: usize| {
            let delims: Vec<char> = delims.chars().collect();
            let word = find_completion_word(input, &delims, cursor);
            (word.text, word.quoting.quote)
        };
        let word = |input: &str, cursor: usize| word_with(" \t\n\"'><=;|&(:", input, cursor);
        let unquoted = |text: &str| (text.to_owned(), None);
        let quoted = |text: &str, q| (text.to_owned(), Some(q));

        assert_eq!(word("", 0), unquoted(""));
        assert_eq!(word("ls fo", 5), unquoted("fo"));
        assert_eq!(word("ls fo", 4), unquoted("f"));
        assert_eq!(word("ls ", 3), unquoted(""));
        assert_eq!(word("ls --opt=", 9), unquoted(""));
        assert_eq!(word("ls --opt=val", 12), unquoted("val"));
        assert_eq!(word("ls --opt=val", 9), unquoted(""));
        assert_eq!(word("ls --opt=val", 8), unquoted("--opt"));
        assert_eq!(word("ls a:=b", 5), unquoted(""));
        assert_eq!(word("ls a\\:b", 7), unquoted("a\\:b"));
        assert_eq!(word("ls \"a b\"c=d", 11), unquoted("d"));
        assert_eq!(word("ls 'a b'c", 9), unquoted("'a b'c"));
        assert_eq!(word("ls é:ü", 8), unquoted("ü"));

        // A break char that's a special prefix starts the word.
        let word = |input: &str, cursor: usize| word_with(" \t\n\"'><=;|&(:@", input, cursor);
        assert_eq!(word("ls a@b", 6), unquoted("@b"));
        assert_eq!(word("ls a:@b", 7), unquoted("@b"));
        assert_eq!(word("ls a@:b", 7), unquoted("b"));
        assert_eq!(word("ls a@", 5), unquoted("@"));

        // In an open quote, the word's text starts after the quote, wherever it opened.
        assert_eq!(word("ls 'a b:c", 9), quoted("a b:c", Quote::Single));
        assert_eq!(word("ls x'a b", 8), quoted("a b", Quote::Single));
        assert_eq!(word("ls --x=\"a", 9), quoted("a", Quote::Double));
        assert_eq!(word(r#"ls "a\"b"#, 8), quoted(r#"a\"b"#, Quote::Double));
        // Backslashes are literal in single quotes, and an escaped quote opens nothing.
        assert_eq!(word(r"ls 'a\'b", 8), unquoted(r"'a\'b"));
        assert_eq!(word(r"ls \'a", 6), unquoted(r"\'a"));
    }

    #[test]
    fn line_words_like_bash() {
        fn words(
            input: &str,
            cursor: usize,
        ) -> (Vec<&str>, Option<usize>, Option<&str>, Option<&str>) {
            let words = find_line_words(input, &[' ', '='], cursor);
            (
                words.comp_words.clone(),
                words.cword,
                words.command_name(),
                words.preceding_word(),
            )
        }

        assert_eq!(words("", 0), (vec![], None, None, None));
        assert_eq!(
            words("ls fo", 5),
            (vec!["ls", "fo"], Some(1), Some("ls"), Some("ls"))
        );
        // The first word's preceding word is itself.
        assert_eq!(
            words("ls", 2),
            (vec!["ls"], Some(0), Some("ls"), Some("ls"))
        );
        // In whitespace, an empty word is inserted at the cursor.
        assert_eq!(
            words("ls  x", 3),
            (vec!["ls", "", "x"], Some(1), Some("ls"), Some("ls"))
        );
        assert_eq!(
            words("ls ", 3),
            (vec!["ls", ""], Some(1), Some("ls"), Some("ls"))
        );
        // A word starting at the cursor wins over one ending there.
        assert_eq!(
            words("ls a=b", 5),
            (vec!["ls", "a", "=", "b"], Some(3), Some("ls"), Some("="))
        );
    }

    #[test]
    fn quoting_before_cursor() {
        let has_quoting =
            |input: &str, cursor| find_completion_word(input, &[' '], cursor).quoting.dequote;

        assert!(!has_quoting("ls a b", 6));
        assert!(has_quoting("ls 'a' b", 8));
        assert!(has_quoting(r"ls a\ b", 7));
        assert!(has_quoting("ls \"a", 5));
        assert!(!has_quoting("ls a 'b'", 4));
    }

    #[test]
    fn completion_tokenization() {
        let words = |input: &str, word_breaks: &str| -> Vec<(String, usize)> {
            let word_breaks: Vec<char> = word_breaks.chars().collect();
            comp_words(input, &word_breaks)
                .into_iter()
                .map(|w| (w.text.to_owned(), w.start))
                .collect()
        };
        let expected = |words: &[(&str, usize)]| -> Vec<(String, usize)> {
            words
                .iter()
                .map(|&(text, start)| (text.to_owned(), start))
                .collect()
        };

        for (input, word_breaks, expected_words) in [
            ("one two", " ", expected(&[("one", 0), ("two", 4)])),
            ("one \t two", " \t", expected(&[("one", 0), ("two", 6)])),
            ("    ", " ", expected(&[])),
            (":", ":", expected(&[(":", 0)])),
            ("a:::b", ": ", expected(&[("a", 0), (":::", 1), ("b", 4)])),
            ("a:=", ":= ", expected(&[("a", 0), (":=", 1)])),
            (
                "a: : :b",
                ": ",
                expected(&[("a", 0), (":", 1), (":", 3), (":", 5), ("b", 6)]),
            ),
            (
                "one two:three",
                ": ",
                expected(&[("one", 0), ("two", 4), (":", 7), ("three", 8)]),
            ),
            ("one'two", "'", expected(&[("one'two", 0)])),
            // Like bash, a quote opens anywhere, even after a word-break char, and ends a
            // run of them.
            (
                "a'b c' x=\"d e",
                " =\"'",
                expected(&[("a'b c'", 0), ("x", 7), ("=", 8), ("\"d e", 9)]),
            ),
            (
                "one 'two:three'",
                ": ",
                expected(&[("one", 0), ("'two:three'", 4)]),
            ),
            (
                "one \\'two \"two four\"",
                ": ",
                expected(&[("one", 0), ("\\'two", 4), ("\"two four\"", 10)]),
            ),
        ] {
            assert_eq!(words(input, word_breaks), expected_words, "{input:?}");
        }
    }
}
