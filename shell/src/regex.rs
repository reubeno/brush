use std::borrow::Cow;

use crate::error;

#[derive(Clone, Debug)]
pub(crate) enum RegexPiece {
    Pattern(String),
    Literal(String),
}

impl RegexPiece {
    fn to_regex_str(&self) -> Cow<str> {
        match self {
            RegexPiece::Pattern(s) => Cow::Borrowed(s.as_str()),
            RegexPiece::Literal(s) => escape_literal_regex_piece(s.as_str()),
        }
    }
}

type RegexWord = Vec<RegexPiece>;

#[derive(Clone, Debug)]
pub struct Regex {
    pieces: RegexWord,
}

impl From<RegexWord> for Regex {
    fn from(pieces: RegexWord) -> Self {
        Self { pieces }
    }
}

impl Regex {
    pub fn matches(&self, value: &str) -> Result<Option<Vec<Option<String>>>, error::Error> {
        let regex_pattern: String = self
            .pieces
            .iter()
            .map(|piece| piece.to_regex_str())
            .collect();

        // TODO: Evaluate how compatible the `fancy_regex` crate is with POSIX EREs.
        let re = fancy_regex::Regex::new(regex_pattern.as_str())?;

        Ok(re.captures(value)?.map(|captures| {
            captures
                .iter()
                .map(|c| c.map(|m| m.as_str().to_owned()))
                .collect()
        }))
    }
}

fn escape_literal_regex_piece(s: &str) -> Cow<str> {
    let mut result = String::new();

    for c in s.chars() {
        match c {
            c if regex_char_is_special(c) => {
                result.push('\\');
                result.push(c);
            }
            c => result.push(c),
        }
    }

    result.into()
}

fn regex_char_is_special(c: char) -> bool {
    matches!(
        c,
        '\\' | '^' | '$' | '.' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
    )
}
