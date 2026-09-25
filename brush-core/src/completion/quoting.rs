//! Completion's quoting model, which follows readline's: finding the quote a line leaves
//! open, dequoting the word being completed, and quoting candidates to replace it, as bash
//! quotes them for readline.
//!
//! It differs from the shell's own quoting in small ways (see [`scan`]). The shell's quote
//! removal is [`brush_parser::unquote_str`], and quoting text as a shell word is in
//! [`escape`](crate::escape).
//!
//! # File names that expand
//!
//! Like bash, file names completing a word show its directory part as typed (e.g. `~/Docs`
//! completing `~/Do`, or `$HOME/Docs` completing `$HOME/Do`), so some name their file only
//! once a leading `~user/` or parameters in them expand ([`ResolvedCandidate::expanded`]).
//! Such a name is quoted so it keeps expanding, as bash quotes it:
//!
//! - Unquoted, a leading `~user/` is left as is. If the rest has parameters or command
//!   substitutions, they're left to expand, and it's double-quoted if anything else needs
//!   quoting (e.g. `"$HOME/Docs dir"`); otherwise it's quoted with backslashes (e.g.
//!   `~/Docs\ dir`).
//! - In a quote, where a `~` can't expand, a name starting with one is replaced with what
//!   it expands to, then quoted as usual (e.g. `"/home/me/Docs dir"`). Parameters are left
//!   to expand in double quotes; in single quotes they're quoted, and don't (e.g.
//!   `'$HOME/Docs dir'`).
//! - The chars left to expand (`$`, `{`, `}`, `(`, `)`, `` ` ``) don't count toward
//!   needing quoting, so e.g. `$HOME/file` isn't quoted.
//!
//! The candidates' common prefix expands like them if it has all of the part of them that
//! expands (e.g. `$HOME/Do` of `$HOME/Docs` and `$HOME/Downloads`, but not `$HO` of
//! `$HOME/a` and `$HOSTS/b`). Like readline, though, it keeps a leading `~` as typed, even
//! in a quote (e.g. `"~/Do`).

use brush_parser::unquote_str;

use super::{CandidateKind, ResolvedCandidate, files::split_tilde_prefix};
use crate::{escape, sys};

/// A quote left open where a word is being completed.
///
/// Like readline, completion only tracks single and double quotes: `$'...'` and `$"..."`
/// open quotes as `'` and `"` do.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(super) enum Quote {
    /// A single quote (`'`).
    Single,
    /// A double quote (`"`).
    Double,
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

    /// Returns the quote's char.
    pub const fn as_char(self) -> char {
        match self {
            Self::Single => '\'',
            Self::Double => '"',
        }
    }
}

impl From<Quote> for escape::QuoteMode {
    fn from(quote: Quote) -> Self {
        match quote {
            Quote::Single => Self::SingleQuote,
            Quote::Double => Self::DoubleQuote,
        }
    }
}

/// How a word that file names are generated for is quoted, which says how to dequote it.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct WordQuoting {
    /// The quote the word being completed is in, if the cursor is inside an unclosed
    /// quote: file names are generated as if the word followed it.
    pub quote: Option<Quote>,
    /// Whether to dequote the word: if anything in the line before the cursor is quoted --
    /// in the word being completed or an earlier one. Like readline, file-name completion
    /// only dequotes a word if so (see the module docs of [`completion`](super) for why
    /// that matters).
    pub dequote: bool,
    /// Whether to dequote the word once more first, taking it as quoted by the caller, as
    /// `compgen` does for a word other than the one being completed.
    pub dequote_again: bool,
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
}

/// Scans `input` for quoting, like readline: quotes and backslash escapes protect the chars
/// they quote. Unlike the shell's, a backslash escapes any char in double quotes.
pub(super) fn scan(input: &str) -> impl Iterator<Item = ScannedChar> + '_ {
    let mut open_quote: Option<(usize, Quote)> = None;
    let mut escaped = false;

    input.char_indices().map(move |(index, c)| {
        let mut unquoted = false;
        if escaped {
            escaped = false;
        } else if let Some((_, q)) = open_quote {
            if c == '\\' && q == Quote::Double {
                escaped = true;
            } else if c == q.as_char() {
                open_quote = None;
            }
        } else if c == '\\' {
            escaped = true;
        } else if let Some(q) = Quote::from_char(c) {
            open_quote = Some((index, q));
        } else {
            unquoted = true;
        }

        ScannedChar {
            index,
            c,
            unquoted,
            open_quote,
        }
    })
}

/// Returns the quote left open at the end of `text`, if any.
pub(super) fn open_quote(text: &str) -> Option<Quote> {
    scan(text)
        .last()
        .and_then(|scanned| scanned.open_quote)
        .map(|(_, q)| q)
}

/// Returns whether `text` has any quoting. Every quote char or backslash in it either
/// quotes something itself or sits in a quote (or after a backslash) that does, so any of
/// them means it has.
pub(super) fn has_quoting(text: &str) -> bool {
    text.contains(['\\', '\'', '"'])
}

/// Dequotes `text`, which starts in `open_quote` if given (e.g. the rest of a word after its
/// opening quote).
pub(super) fn unquote_in_quote(text: &str, open_quote: Option<Quote>) -> String {
    match open_quote {
        Some(q) => unquote_str(&std::format!("{}{text}", q.as_char())),
        None => unquote_str(text),
    }
}

/// Dequotes `word` to match it against a list of names, like bash: a leading quote is
/// dropped as if it closed a quote, so a quote char after it opens one. That way a word
/// from `COMP_WORDS` (e.g. bash-completion's `$cur`), which starts with the quote the word
/// being completed is in, matches the names that the rest of it starts.
pub(super) fn dequote_for_matching(word: &str) -> String {
    let word = word.strip_prefix(['\'', '"']).unwrap_or(word);
    unquote_str(word)
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
    /// to suit the quote the word is in, if any, and so that expansions in it that name the
    /// file keep working (e.g. `~/` or `$HOME/`). Anything else is inserted as is, after the
    /// word's opening quote.
    pub fn quote(&self, candidate: &ResolvedCandidate, add_slash: bool) -> QuotedCandidate {
        let opening_quote = self.open_quote.map(Quote::as_char);

        if !matches!(candidate.kind, CandidateKind::FileName { .. }) {
            return QuotedCandidate {
                text: opening_quote
                    .into_iter()
                    .chain(candidate.text.chars())
                    .collect(),
                closed: None,
            };
        }

        // Quote the name, and put a directory's slash after it.
        let name = sys::fs::strip_path_separator_suffix(&candidate.text);
        let mut slash = candidate.text.strip_prefix(name).unwrap_or_default();
        if add_slash && slash.is_empty() {
            slash = "/";
        }

        // See "File names that expand" in the module docs.
        let (name, expands) = match candidate.expanded.as_deref() {
            Some(expanded) if self.open_quote.is_some() && name.starts_with('~') => {
                (sys::fs::strip_path_separator_suffix(expanded), false)
            }
            expanded => (name, expanded.is_some()),
        };

        let quoted = self
            .quote_file_names
            .then(|| quote_file_name(name, expands, self.open_quote))
            .flatten();
        match (self.open_quote, quoted) {
            (None, quoted) => QuotedCandidate {
                text: std::format!("{}{slash}", quoted.as_deref().unwrap_or(name)),
                closed: None,
            },
            (Some(q), Some(closed)) => {
                let open = closed.strip_suffix(q.as_char()).unwrap_or(&closed);
                QuotedCandidate {
                    text: std::format!("{open}{slash}"),
                    closed: Some(std::format!("{closed}{slash}")),
                }
            }
            (Some(q), None) => QuotedCandidate {
                text: std::format!("{}{name}{slash}", q.as_char()),
                closed: None,
            },
        }
    }
}

/// Quotes `name`, a file name, if it needs quoting in `open_quote`: in full, in that quote,
/// or with backslashes if there's none. Returns `None` if it doesn't need quoting.
///
/// If it `expands`, it's quoted so it still does, as the module docs describe.
fn quote_file_name(name: &str, expands: bool, open_quote: Option<Quote>) -> Option<String> {
    let (tilde, rest) = if expands {
        split_tilde_prefix(name)
    } else {
        ("", name)
    };
    // A bare `~user/` needs no quoting, though as a word on its own, its empty rest would.
    if expands && rest.is_empty() {
        return None;
    }
    let params_expand = expands && rest.contains(['$', '`']);

    let needs_quoting = if params_expand {
        escape::needs_quoting(&rest.replace(['$', '{', '}', '(', ')', '`'], ""))
    } else {
        escape::needs_quoting(rest)
    };
    if !needs_quoting {
        return None;
    }

    let quoted = if params_expand && open_quote != Some(Quote::Single) {
        escape::double_quote_leaving(rest, &['$', '`'])
    } else {
        let mode = open_quote.map_or(escape::QuoteMode::BackslashEscape, Into::into);
        escape::force_quote(rest, mode)
    };
    Some(std::format!("{tilde}{quoted}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_quote_at_end() {
        assert_eq!(open_quote("abc"), None);
        assert_eq!(open_quote("'a b"), Some(Quote::Single));
        assert_eq!(open_quote(r#""a\"b"#), Some(Quote::Double));
        assert_eq!(open_quote("'a'b\"c"), Some(Quote::Double));
        assert_eq!(open_quote(r"a\'b"), None);
        assert_eq!(open_quote("'a\"b'"), None);
    }

    #[test]
    fn unquote_text_in_open_quote() {
        assert_eq!(unquote_in_quote(r"a\b", None), "ab");
        assert_eq!(unquote_in_quote(r"a\b", Some(Quote::Single)), r"a\b");
        assert_eq!(unquote_in_quote(r#"a"b"#, Some(Quote::Single)), r#"a"b"#);
        assert_eq!(unquote_in_quote(r#"a\"b"#, Some(Quote::Double)), r#"a"b"#);
        assert_eq!(unquote_in_quote(r"a\ b", Some(Quote::Double)), r"a\ b");

        // The open quote can close, and others open.
        assert_eq!(
            unquote_in_quote(r#"a'b"c\d"#, Some(Quote::Single)),
            r"abc\d"
        );
        assert_eq!(
            unquote_in_quote(r#"\"a\'b/"#, Some(Quote::Double)),
            r#""a\'b/"#
        );
    }

    #[test]
    fn dequote_for_matching_like_bash() {
        assert_eq!(dequote_for_matching(r"a\'b"), "a'b");
        assert_eq!(dequote_for_matching(r#""a b"#), "a b");
        // A leading quote is dropped rather than opening a quote, so backslashes after it
        // escape, and a quote char after it opens a new quote.
        assert_eq!(dequote_for_matching(r"'a\b'"), "ab");
        assert_eq!(dequote_for_matching(r#"'a"b'"#), "ab'");
        assert_eq!(dequote_for_matching(r#""a\\"#), r"a\");
    }

    fn file_name_quoter(quote: Option<Quote>) -> Quoter {
        Quoter {
            open_quote: quote,
            quote_file_names: true,
        }
    }

    /// Like readline, a file name that names a file only once expanded keeps expanding
    /// when quoted. Expected values were captured from bash 5.3.
    #[test]
    fn expanding_file_names_keep_expanding() {
        let quote = |text: &str, expanded: &str, quote| {
            let candidate = ResolvedCandidate {
                expanded: Some(expanded.to_owned()),
                ..ResolvedCandidate::file_name(text)
            };
            let quoted = file_name_quoter(quote).quote(&candidate, false);
            quoted.closed.unwrap_or(quoted.text)
        };

        for (text, expanded, q, expected) in [
            ("~/Docs dir/", "/h/Docs dir/", None, r"~/Docs\ dir/"),
            ("~/plainfile", "/h/plainfile", None, "~/plainfile"),
            // Nothing after the `~/`, so nothing to quote.
            ("~/", "/h/", None, "~/"),
            (
                "$HOME/Docs dir/",
                "/h/Docs dir/",
                None,
                r#""$HOME/Docs dir"/"#,
            ),
            ("$HOME/plainfile", "/h/plainfile", None, "$HOME/plainfile"),
            (
                "${HOME}/plainfile",
                "/h/plainfile",
                None,
                "${HOME}/plainfile",
            ),
            // In quotes, parameters expand only in double quotes, and a `~` in neither.
            (
                "$HOME/Docs dir/",
                "/h/Docs dir/",
                Some(Quote::Double),
                r#""$HOME/Docs dir"/"#,
            ),
            (
                "$HOME/Docs dir/",
                "/h/Docs dir/",
                Some(Quote::Single),
                "'$HOME/Docs dir'/",
            ),
            (
                "~/Docs dir/",
                "/h/Docs dir/",
                Some(Quote::Double),
                r#""/h/Docs dir"/"#,
            ),
            (
                "~/Docs dir/",
                "/h/Docs dir/",
                Some(Quote::Single),
                "'/h/Docs dir'/",
            ),
        ] {
            assert_eq!(quote(text, expanded, q), expected, "{text:?} in {q:?}");
        }

        // A file named literally is quoted as usual.
        assert_eq!(
            file_name_quoter(None).quote(&ResolvedCandidate::file_name("a$b"), false),
            QuotedCandidate {
                text: r"a\$b".to_owned(),
                closed: None
            }
        );
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
