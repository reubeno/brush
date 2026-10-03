//! Completion's quoting model, which follows readline's: finding the quotes in a line
//! being completed.
//!
//! It differs from the shell's own quoting in small ways (see [`scan`]).

/// A quote left open where a word is being completed.
///
/// Unlike readline, which knows only single and double quotes, completion knows `$'...'`
/// too. `$"..."` opens a quote as `"` does.
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
}
