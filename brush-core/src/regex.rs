use std::borrow::Cow;

use crate::error;

/// Represents a piece of a regular expression.
#[derive(Clone, Debug)]
pub(crate) enum RegexPiece {
    /// A pattern that should be interpreted as a regular expression.
    Pattern(String),
    /// A literal string that should be matched exactly.
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

/// Encapsulates a regular expression usable in the shell.
#[derive(Clone, Debug)]
pub struct Regex {
    pieces: RegexWord,
    case_insensitive: bool,
}

impl From<RegexWord> for Regex {
    fn from(pieces: RegexWord) -> Self {
        Self {
            pieces,
            case_insensitive: false,
        }
    }
}

impl Regex {
    /// Sets the regular expression's case sensitivity.
    ///
    /// # Arguments
    ///
    /// * `value` - The new case sensitivity value.
    pub fn set_case_insensitive(mut self, value: bool) -> Self {
        self.case_insensitive = value;
        self
    }

    /// Computes if the regular expression matches the given string.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to check for a match.
    pub fn matches(&self, value: &str) -> Result<Option<Vec<Option<String>>>, error::Error> {
        let regex_pattern: String = self
            .pieces
            .iter()
            .map(|piece| piece.to_regex_str())
            .collect();

        // TODO: Evaluate how compatible the `fancy_regex` crate is with POSIX EREs.
        let re = compile_regex(regex_pattern, self.case_insensitive)?;

        Ok(re.captures(value)?.map(|captures| {
            captures
                .iter()
                .map(|c| c.map(|m| m.as_str().to_owned()))
                .collect()
        }))
    }
}

#[allow(clippy::needless_pass_by_value)]
#[cached::proc_macro::cached(size = 64, result = true)]
pub(crate) fn compile_regex(
    regex_str: String,
    case_insensitive: bool,
) -> Result<fancy_regex::Regex, error::Error> {
    let mut builder = fancy_regex::RegexBuilder::new(regex_str.as_str());
    builder.case_insensitive(case_insensitive);

    match builder.build() {
        Ok(re) => Ok(re),
        Err(e) => Err(error::Error::InvalidRegexError(e, regex_str)),
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
