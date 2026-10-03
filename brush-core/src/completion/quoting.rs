//! Completion's quoting model, which follows readline's: finding the quotes in a line
//! being completed.
//!
//! It differs from the shell's own quoting in small ways (see [`scan`]).

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
