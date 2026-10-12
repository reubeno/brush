//! Completion's quoting model, which follows readline's: finding the quote a line leaves
//! open, and quoting candidates to replace the word being completed, as bash quotes them
//! for readline.
//!
//! It differs from the shell's own quoting in small ways (see [`scan`]), and from readline's
//! in knowing `$'...'` quotes ([`Quote::AnsiC`]). Quoting text as a shell word is in
//! [`escape`].

use super::{CandidateKind, ResolvedCandidate};
use crate::{escape, sys};

/// A quote left open where a word is being completed.
///
/// Unlike readline, which knows only single and double quotes, completion knows `$'...'`
/// too, so it can complete in one (e.g. a name with a newline). `$"..."` opens a quote as
/// `"` does.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(super) enum Quote {
    /// A single quote (`'`).
    Single,
    /// A double quote (`"`).
    Double,
    /// An ANSI-C quote (`$'...'`), in which a backslash escapes any char.
    AnsiC,
}

impl Quote {
    /// Returns the quote that `c` opens, if it's a quote char.
    const fn from_char(c: char) -> Option<Self> {
        match c {
            '\'' => Some(Self::Single),
            '"' => Some(Self::Double),
            _ => None,
        }
    }

    /// Returns the char that closes the quote.
    pub const fn as_char(self) -> char {
        match self {
            Self::Single | Self::AnsiC => '\'',
            Self::Double => '"',
        }
    }

    /// Returns the text that opens the quote.
    pub const fn opening(self) -> &'static str {
        match self {
            Self::Single => "'",
            Self::Double => "\"",
            Self::AnsiC => "$'",
        }
    }
}

impl From<Quote> for escape::QuoteMode {
    fn from(quote: Quote) -> Self {
        match quote {
            Quote::Single => Self::SingleQuote,
            Quote::Double => Self::DoubleQuote,
            Quote::AnsiC => Self::AnsiC,
        }
    }
}

/// How a word being completed is quoted.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WordQuoting {
    /// The quote the word being completed is in, if the cursor is inside an unclosed
    /// quote.
    pub quote: Option<Quote>,
}

/// A char of a line, as quoting affects it (see [`scan`]).
pub(super) struct ScannedChar {
    /// The byte offset of the char.
    pub index: usize,
    pub c: char,
    /// Whether the char is unquoted, and doesn't quote anything itself (as a quote char
    /// or a backslash does): only such a char can break words.
    pub unquoted: bool,
    /// The quote left open after this char, if any, and its byte offset.
    pub open_quote: Option<(usize, Quote)>,
    /// Whether the char is a backslash that escapes the char after it.
    pub escapes_next: bool,
}

/// Scans `input` for quoting, like readline: quotes and backslash escapes protect the chars
/// they quote. Unlike the shell's, a backslash escapes any char in double quotes. Unlike
/// readline's, an unquoted `$` before a `'` opens a `$'...'` quote -- unless it ends a `$$`.
pub(super) fn scan(input: &str) -> impl Iterator<Item = ScannedChar> + '_ {
    let mut open_quote: Option<(usize, Quote)> = None;
    let mut escaped = false;
    // The offset of the char before, if it's an unquoted `$` that could open a `$'...'`.
    let mut dollar = None;

    input.char_indices().map(move |(index, c)| {
        let mut unquoted = false;
        let after_dollar = dollar.take();
        if escaped {
            escaped = false;
        } else if let Some((_, q)) = open_quote {
            if c == '\\' && q != Quote::Single {
                escaped = true;
            } else if c == q.as_char() {
                open_quote = None;
            }
        } else if c == '\\' {
            escaped = true;
        } else if let Some(q) = Quote::from_char(c) {
            open_quote = Some(match (q, after_dollar) {
                (Quote::Single, Some(dollar)) => (dollar, Quote::AnsiC),
                _ => (index, q),
            });
        } else {
            unquoted = true;
            // A `$` after one that could open a quote makes `$$` with it instead.
            if c == '$' && after_dollar.is_none() {
                dollar = Some(index);
            }
        }

        ScannedChar {
            index,
            c,
            unquoted,
            open_quote,
            escapes_next: escaped,
        }
    })
}

/// Ends the quoting `text` leaves open, so it parses: drops a backslash left at the end with
/// nothing to escape, then closes the quote left open, if any.
pub(super) fn close(text: &mut String) {
    let Some(last) = scan(text).last() else {
        return;
    };
    if last.escapes_next {
        text.pop();
    }
    if let Some((_, q)) = last.open_quote {
        text.push(q.as_char());
    }
}

/// A candidate quoted to replace the word being completed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct QuotedCandidate {
    /// The candidate, quoted as needed, starting with the quote the word is in (if any)
    /// and leaving that quote open: e.g. `"dir with space/`.
    pub text: String,
    /// If the candidate needed quoting in the quote the word is in, the candidate with that
    /// quote closed, before any trailing `/`: e.g. `"dir with space"/`.
    pub closed: Option<String>,
}

/// Quotes candidates to replace the word being completed, as bash quotes them for readline.
pub(super) struct Quoter {
    /// The quote the word being completed is in, if any.
    pub open_quote: Option<Quote>,
    /// Whether to quote candidates that are file names: unless the spec said not to
    /// ([`CompleteOption::NoQuote`](super::CompleteOption::NoQuote)).
    pub quote_file_names: bool,
}

impl Quoter {
    /// Quotes `candidate` to replace the word being completed, adding a `/` to mark a
    /// directory if `add_slash` and it doesn't end with one.
    ///
    /// Like readline, a file name is quoted (unless [`Self::quote_file_names`] says not to)
    /// to suit the quote the word is in, if any. Anything else is inserted as is, after the
    /// word's opening quote.
    pub fn quote(&self, candidate: &ResolvedCandidate, add_slash: bool) -> QuotedCandidate {
        if !matches!(candidate.kind, CandidateKind::FileName { .. }) {
            let opening_quote = self.open_quote.map_or("", Quote::opening);
            return QuotedCandidate {
                text: std::format!("{opening_quote}{}", candidate.text),
                closed: None,
            };
        }

        // Quote the name, and put a directory's slash after it.
        let name = sys::fs::strip_path_separator_suffix(&candidate.text);
        let mut slash = candidate.text.strip_prefix(name).unwrap_or_default();
        if add_slash && slash.is_empty() {
            slash = "/";
        }

        let quoted = self
            .quote_file_names
            .then(|| quote_file_name(name, self.open_quote))
            .flatten();
        match (self.open_quote, quoted) {
            (None, quoted) => QuotedCandidate {
                text: std::format!("{}{slash}", quoted.as_deref().unwrap_or(name)),
                closed: None,
            },
            (Some(q), Some(closed)) => {
                // Leave the quote open, unless the name ends outside of it (e.g. a lone `'`,
                // quoted `\'`, in single quotes): then it's written in full.
                let open = if ends_in_quote(&closed) {
                    closed.strip_suffix(q.as_char()).unwrap_or(&closed)
                } else {
                    &closed
                };
                QuotedCandidate {
                    text: std::format!("{open}{slash}"),
                    closed: Some(std::format!("{closed}{slash}")),
                }
            }
            (Some(q), None) => QuotedCandidate {
                text: std::format!("{}{name}{slash}", q.opening()),
                closed: None,
            },
        }
    }
}

/// Returns whether `text`'s last char closes a quote.
fn ends_in_quote(text: &str) -> bool {
    let (mut before_last, mut last) = (None, None);
    for scanned in scan(text) {
        before_last = last;
        last = Some(scanned.open_quote);
    }
    matches!((before_last, last), (Some(Some(_)), Some(None)))
}

/// Quotes `name`, a file name, if it needs quoting in `open_quote`: in full, in that quote,
/// or with backslashes if there's none. Returns `None` if it doesn't need quoting.
fn quote_file_name(name: &str, open_quote: Option<Quote>) -> Option<String> {
    // Like bash quoting a file name it completes, leave a `~` unquoted, so that a `~user`
    // completion still expands.
    let options = |mode| {
        escape::QuoteOptions::builder()
            .preferred_mode(mode)
            .leave_tilde(true)
            .build()
    };
    if escape::quote(name, &options(escape::QuoteMode::BackslashEscape)) == name {
        return None;
    }

    let mode = open_quote.map_or(escape::QuoteMode::BackslashEscape, Into::into);
    Some(escape::quote(name, &options(mode)).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_ends_open_quoting() {
        let closed = |text: &str| {
            let mut text = text.to_owned();
            close(&mut text);
            text
        };

        assert_eq!(closed(""), "");
        assert_eq!(closed("abc"), "abc");
        assert_eq!(closed("'a b"), "'a b'");
        assert_eq!(closed(r#""a\"b"#), r#""a\"b""#);
        // A backslash with nothing to escape is dropped, unless it's literal in single
        // quotes; an escaped one stays.
        assert_eq!(closed(r"a\"), "a");
        assert_eq!(closed(r#""a\"#), r#""a""#);
        assert_eq!(closed(r"'a\"), r"'a\'");
        assert_eq!(closed(r"a\\"), r"a\\");
        assert_eq!(closed(r#""a\\"#), r#""a\\""#);

        // In `$'...'`, a backslash escapes any char, a quote too; `$$` is no `$'`.
        assert_eq!(closed(r"$'a\'b"), r"$'a\'b'");
        assert_eq!(closed(r"$'a\"), r"$'a'");
        assert_eq!(closed(r"$$'a\'b"), r"$$'a\'b");
        assert_eq!(closed(r"$$$'a"), r"$$$'a'");
        assert_eq!(closed(r#""$'a"#), r#""$'a""#);
    }

    #[test]
    fn scan_finds_ansi_c_quotes() {
        let open_quote = |text: &str| scan(text).last().and_then(|sc| sc.open_quote);

        assert_eq!(open_quote("$'a"), Some((0, Quote::AnsiC)));
        assert_eq!(open_quote("x$'a"), Some((1, Quote::AnsiC)));
        assert_eq!(open_quote(r"\$'a"), Some((2, Quote::Single)));
        assert_eq!(open_quote("$$'a"), Some((2, Quote::Single)));
        assert_eq!(open_quote("$$$'a"), Some((2, Quote::AnsiC)));
        assert_eq!(open_quote("$$$$'a"), Some((4, Quote::Single)));
        assert_eq!(open_quote("$\"a"), Some((1, Quote::Double)));
    }

    #[test]
    fn file_names_are_quoted_like_bash() {
        // Like bash, a `~user` file name isn't quoted, so it still expands...
        assert_eq!(quote_file_name("~root", None), None);
        // ...but a leading `#` is, so it doesn't start a comment.
        assert_eq!(quote_file_name("#hash", None).as_deref(), Some(r"\#hash"));
        assert_eq!(quote_file_name("a b", None).as_deref(), Some(r"a\ b"));
        // A comma isn't special on its own.
        assert_eq!(quote_file_name("a,b", None), None);
    }

    fn file_name_quoter(quote: Option<Quote>) -> Quoter {
        Quoter {
            open_quote: quote,
            quote_file_names: true,
        }
    }

    /// Like readline, a file name in an open quote is quoted to suit it: in full, if it
    /// needs quoting, and otherwise as is. Expected values were captured from bash 5.3.
    #[test]
    fn file_names_in_open_quote() {
        for (text, q, expected_text, expected_closed) in [
            ("it's", Quote::Double, r#""it's"#, Some(r#""it's""#)),
            ("it's", Quote::Single, r"'it'\''s", Some(r"'it'\''s'")),
            (
                "dir with space/",
                Quote::Double,
                r#""dir with space/"#,
                Some(r#""dir with space"/"#),
            ),
            ("plain", Quote::Double, r#""plain"#, None),
            ("sub/", Quote::Single, "'sub/", None),
        ] {
            let quoted =
                file_name_quoter(Some(q)).quote(&ResolvedCandidate::file_name(text), false);
            assert_eq!(quoted.text, expected_text, "{text:?} in {q:?}");
            assert_eq!(
                quoted.closed.as_deref(),
                expected_closed,
                "{text:?} in {q:?}"
            );
        }
    }

    #[test]
    fn candidates_that_are_not_file_names_are_not_quoted() {
        let quoter = Quoter {
            open_quote: Some(Quote::Double),
            quote_file_names: true,
        };
        assert_eq!(
            quoter.quote(&ResolvedCandidate::new("a b"), false),
            QuotedCandidate {
                text: "\"a b".to_owned(),
                closed: None
            }
        );
    }
}
