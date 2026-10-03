//! Completion's quoting model, which follows readline's: finding the quote a line leaves
//! open, dequoting the word being completed, and quoting candidates to replace it, as bash
//! quotes them for readline.
//!
//! It differs from the shell's own quoting in small ways (see [`scan`]), and from readline's
//! in knowing `$'...'` quotes ([`Quote::AnsiC`]). The shell's quote removal is
//! [`brush_parser::unquote_str`] ([`unquote`] adds `$'...'` to it), and quoting text as a
//! shell word is in [`escape`].
//!
//! # File names that expand
//!
//! Like bash, file names completing a word show its directory part as typed (e.g. `~/Docs`
//! completing `~/Do`, or `$HOME/Docs` completing `$HOME/Do`), so some name their file only
//! once a leading `~user/` or parameters in them expand ([`ResolvedCandidate::expanded`]).
//! Such a name is quoted so it keeps expanding, as bash quotes it:
//!
//! - Unquoted, a leading `~user/` is left as is. If the rest of the directory part has
//!   parameters or command substitutions, they're left to expand, and it's double-quoted if
//!   anything else needs quoting (e.g. `"$HOME/Docs dir"`); otherwise it's quoted with
//!   backslashes (e.g. `~/Docs\ dir`).
//! - Only the directory part, as typed, expands: a `$` or `` ` `` in the file name itself is
//!   quoted (e.g. `"$HOME/a\$b"`). Unlike bash 5.3, which leaves those to expand too, so
//!   the name names another file, or runs a command.
//! - In a quote, where a `~` can't expand, a name starting with one is replaced with what
//!   it expands to, then quoted as usual (e.g. `"/home/me/Docs dir"`). Parameters are left
//!   to expand in double quotes; in single quotes they're quoted, and don't (e.g.
//!   `'$HOME/Docs dir'`).
//! - The chars left to expand in the directory part (`$`, `{`, `}`, `(`, `)`, `` ` ``)
//!   don't count toward needing quoting, so e.g. `$HOME/file` isn't quoted.
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

/// Removes quoting from `text` as the shell does: like [`unquote_str`], but translating the
/// escapes in a `$'...'` quote too (which the shell does when it parses a word).
pub(super) fn unquote(text: &str) -> String {
    let translate = |quoted: &str| {
        escape::expand_backslash_escapes(quoted, escape::EscapeExpansionMode::AnsiCQuotes)
            .map_or_else(
                |_| quoted.to_owned(),
                |(bytes, _)| String::from_utf8_lossy(&bytes).into_owned(),
            )
    };
    let slice = |range: std::ops::Range<usize>| text.get(range).unwrap_or_default();

    let mut result = String::with_capacity(text.len());
    // Where the text since the last `$'...'` quote starts, and the contents of an open one.
    let mut unquoted_start = 0;
    let mut ansi_c_start = None;
    for scanned in scan(text) {
        match (ansi_c_start, scanned.open_quote) {
            (None, Some((dollar, Quote::AnsiC))) => {
                result.push_str(&unquote_str(slice(unquoted_start..dollar)));
                ansi_c_start = Some(scanned.index + 1);
            }
            (Some(start), None) => {
                result.push_str(&translate(slice(start..scanned.index)));
                ansi_c_start = None;
                unquoted_start = scanned.index + 1;
            }
            _ => (),
        }
    }
    match ansi_c_start {
        Some(start) => result.push_str(&translate(slice(start..text.len()))),
        None => result.push_str(&unquote_str(slice(unquoted_start..text.len()))),
    }
    result
}

/// Dequotes `text`, which starts in `open_quote` if given (e.g. the rest of a word after its
/// opening quote).
pub(super) fn unquote_in_quote(text: &str, open_quote: Option<Quote>) -> String {
    match open_quote {
        Some(q) => unquote(&std::format!("{}{text}", q.opening())),
        None => unquote(text),
    }
}

/// Dequotes `word` to match it against a list of names, like bash: a leading quote is
/// dropped as if it closed a quote, so a quote char after it opens one. That way a word
/// from `COMP_WORDS` (e.g. bash-completion's `$cur`), which starts with the quote the word
/// being completed is in, matches the names that the rest of it starts.
pub(super) fn dequote_for_matching(word: &str) -> String {
    let word = word.strip_prefix(['\'', '"']).unwrap_or(word);
    unquote(word)
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

        // See "File names that expand" in the module docs.
        let (name, expanded) = match candidate.expanded.as_deref() {
            Some(expanded) if self.open_quote.is_some() && name.starts_with('~') => {
                (sys::fs::strip_path_separator_suffix(expanded), None)
            }
            expanded => (name, expanded.map(sys::fs::strip_path_separator_suffix)),
        };

        let quoted = self
            .quote_file_names
            .then(|| quote_file_name(name, expanded, self.open_quote))
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
///
/// If it's `expanded` (to name the file), it's quoted so it still expands, as the module
/// docs describe.
fn quote_file_name(
    name: &str,
    expanded: Option<&str>,
    open_quote: Option<Quote>,
) -> Option<String> {
    let expands = expanded.is_some();
    let (tilde, rest) = if expands {
        split_tilde_prefix(name)
    } else {
        ("", name)
    };
    // A bare `~user/` needs no quoting, though as a word on its own, its empty rest would.
    if expands && rest.is_empty() {
        return None;
    }
    // Only the directory part, as typed, can expand. The file name after it is literal, so
    // it's unchanged at the end of what the name expands to; if there's no such name (e.g.
    // in `$HOME/`), it's all directory part.
    let last = rest.rsplit_once('/').map_or(rest, |(_, last)| last);
    let (dir, file) = match expanded {
        Some(expanded) if expanded.ends_with(last) => rest.split_at(rest.len() - last.len()),
        Some(_) => (rest, ""),
        None => ("", rest),
    };
    let params_expand = dir.contains(['$', '`']);

    // Like bash quoting a file name it completes, leave a `~` unquoted, so that a `~user`
    // completion still expands.
    let options = |mode| {
        escape::QuoteOptions::builder()
            .preferred_mode(mode)
            .leave_tilde(true)
            .build()
    };
    let needs_quoting =
        |s: &str| escape::quote(s, &options(escape::QuoteMode::BackslashEscape)) != s;
    let needs_quoting = if params_expand {
        needs_quoting(&dir.replace(['$', '{', '}', '(', ')', '`'], ""))
            || (!file.is_empty() && needs_quoting(file))
    } else {
        needs_quoting(rest)
    };
    if !needs_quoting {
        return None;
    }

    let quoted = if params_expand && !matches!(open_quote, Some(Quote::Single | Quote::AnsiC)) {
        // One double-quoted word, with the directory part's expansions left live.
        let dir = escape::double_quote_leaving(dir, &['$', '`']);
        let file = escape::double_quote_leaving(file, &[]);
        std::format!(
            "{}{}",
            dir.strip_suffix('"').unwrap_or(&dir),
            file.strip_prefix('"').unwrap_or(&file)
        )
    } else {
        let mode = open_quote.map_or(escape::QuoteMode::BackslashEscape, Into::into);
        escape::quote(rest, &options(mode)).into_owned()
    };
    Some(std::format!("{tilde}{quoted}"))
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
        assert_eq!(quote_file_name("~root", None, None), None);
        // ...but a leading `#` is, so it doesn't start a comment.
        assert_eq!(
            quote_file_name("#hash", None, None).as_deref(),
            Some(r"\#hash")
        );
        assert_eq!(quote_file_name("a b", None, None).as_deref(), Some(r"a\ b"));
        // A comma isn't special on its own.
        assert_eq!(quote_file_name("a,b", None, None), None);
    }

    #[test]
    fn open_quote_at_end() {
        assert_eq!(open_quote("abc"), None);
        assert_eq!(open_quote("'a b"), Some(Quote::Single));
        assert_eq!(open_quote(r#""a\"b"#), Some(Quote::Double));
        assert_eq!(open_quote("'a'b\"c"), Some(Quote::Double));
        assert_eq!(open_quote(r"a\'b"), None);
        assert_eq!(open_quote("'a\"b'"), None);

        // An unquoted `$` before a `'` opens a `$'...'` quote, where a backslash escapes.
        assert_eq!(open_quote("$'a"), Some(Quote::AnsiC));
        assert_eq!(open_quote(r"$'a\'b"), Some(Quote::AnsiC));
        assert_eq!(open_quote(r"$'a\'b'"), None);
        assert_eq!(open_quote(r"\$'a"), Some(Quote::Single));
        assert_eq!(open_quote("\"$'a"), Some(Quote::Double));
    }

    #[test]
    fn unquote_translates_ansi_c_quotes() {
        assert_eq!(unquote(r"$'a\nb'"), "a\nb");
        assert_eq!(unquote(r"x$'a\'b'y"), "xa'by");
        assert_eq!(unquote(r#"'a'"b"$'\t'c\ d"#), "ab\tc d");
        // Left open, it runs to the end.
        assert_eq!(unquote(r"$'a\nb"), "a\nb");
        // A `$` that's quoted or escaped doesn't open one.
        assert_eq!(unquote("\"$'a'\""), "$'a'");
        assert_eq!(unquote(r"\$'a'"), "$a");
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
            // Like bash, a `(` in the file name is quoted.
            ("$HOME/d(e)", "/h/d(e)", None, r#""$HOME/d(e)""#),
            // With no file name past the directory part, it all still expands.
            ("$HOME/", "/h/", None, "$HOME/"),
            ("${HOME}/", "/h/", None, "${HOME}/"),
        ] {
            assert_eq!(quote(text, expanded, q), expected, "{text:?} in {q:?}");
        }

        // Only the directory part, as typed, keeps expanding: a `$` or `` ` `` in the file
        // name itself is quoted. (Unlike bash 5.3, which leaves those to expand too, so the
        // name names another file, or runs a command.)
        for (text, expanded, q, expected) in [
            ("$HOME/a$b", "/h/a$b", None, r#""$HOME/a\$b""#),
            (
                "$HOME/c$(printf marker)",
                "/h/c$(printf marker)",
                None,
                r#""$HOME/c\$(printf marker)""#,
            ),
            ("$HOME/x`y", "/h/x`y", None, r#""$HOME/x\`y""#),
            (
                "$HOME/Docs dir/f$g",
                "/h/Docs dir/f$g",
                None,
                r#""$HOME/Docs dir/f\$g""#,
            ),
            (
                "$HOME/a$b",
                "/h/a$b",
                Some(Quote::Double),
                r#""$HOME/a\$b""#,
            ),
            ("$HOME/a$b", "/h/a$b", Some(Quote::Single), "'$HOME/a$b'"),
            ("~/a$b", "/h/a$b", None, r"~/a\$b"),
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
