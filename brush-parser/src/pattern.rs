//! Implements parsing for shell glob and extglob patterns.

use crate::{error, word::WordString};

/// Represents the kind of an extended glob.
pub enum ExtendedGlobKind {
    /// The `+` extended glob; matches one or more occurrences of the inner pattern.
    Plus,
    /// The `@` extended glob; allows matching an alternation of inner patterns.
    At,
    /// The `!` extended glob; matches the negation of the inner pattern.
    Exclamation,
    /// The `?` extended glob; matches zero or one occurrence of the inner pattern.
    Question,
    /// The `*` extended glob; matches zero or more occurrences of the inner pattern.
    Star,
}

/// Converts a shell pattern to a regular expression string.
///
/// # Arguments
///
/// * `pattern` - The shell pattern to convert.
/// * `enable_extended_globbing` - Whether to enable extended globbing (extglob).
pub fn pattern_to_regex_str(
    pattern: &WordString,
    enable_extended_globbing: bool,
) -> Result<WordString, error::WordParseError> {
    pattern_to_regex_translator::pattern(pattern, enable_extended_globbing)
        .map_err(error::WordParseError::Pattern)
}

peg::parser! {
    grammar pattern_to_regex_translator(enable_extended_globbing: bool) for WordString {
        pub(crate) rule pattern() -> WordString =
            pieces:(pattern_piece()*) {
                pieces.join("").into()
            }

        rule pattern_piece() -> WordString =
            escape_sequence() /
            bracket_expression() /
            extglob_enabled() s:extended_glob_pattern() { s } /
            wildcard() /
            [c if regex_char_needs_escaping(c)] {
                let mut s: WordString = '\\'.into();
                s.push(c);
                s
            } /
            [c] { c.into() }

        rule escape_sequence() -> WordString =
            sequence:$(['\\'] [c if regex_char_needs_escaping(c)]) { sequence } /
            ['\\'] [c] { c.into() }

        rule bracket_expression() -> WordString =
            "[" invert:(("!")?) members:bracket_member()+ "]" {
                let mut members = members;
                if invert.is_some() {
                    members.insert(0, WordString::from("^"));
                }

                std::format!("[{}]", members.join("")).into()
            }

        rule bracket_member() -> WordString =
            char_class_expression() /
            char_range() /
            char_list()

        rule char_class_expression() -> WordString =
            e:$("[:" char_class() ":]") { e }

        rule char_class() =
            "alnum" / "alpha" / "blank" / "cntrl" / "digit" / "graph" / "lower" / "print" / "punct" / "space" / "upper"/ "xdigit"

        rule char_range() -> WordString =
            range:$([_] "-" [_]) { range }

        rule char_list() -> WordString =
            chars:$([c if c != ']']+) { escape_char_class_char_list(&chars) }

        rule wildcard() -> WordString =
            "?" { WordString::from(".") } /
            "*" { WordString::from(".*") }

        rule extglob_enabled() -> () =
            &[_] {? if enable_extended_globbing { Ok(()) } else { Err("extglob disabled") } }

        pub(crate) rule extended_glob_pattern() -> WordString =
            kind:extended_glob_prefix() "(" branches:extended_glob_body() ")" {
                let mut s = WordString::new();

                s.push('(');

                // fancy_regex uses ?! to indicate a negative lookahead.
                if matches!(kind, ExtendedGlobKind::Exclamation) {
                    s.push_str("(?!");
                }

                s.push_str(&branches.join("|"));
                s.push(')');

                match kind {
                    ExtendedGlobKind::Plus => s.push('+'),
                    ExtendedGlobKind::Question => s.push('?'),
                    ExtendedGlobKind::Star => s.push('*'),
                    ExtendedGlobKind::At | ExtendedGlobKind::Exclamation => (),
                }

                if matches!(kind, ExtendedGlobKind::Exclamation) {
                    s.push_str(".)*?");
                }

                s
            }

        rule extended_glob_prefix() -> ExtendedGlobKind =
            "+" { ExtendedGlobKind::Plus } /
            "@" { ExtendedGlobKind::At } /
            "!" { ExtendedGlobKind::Exclamation } /
            "?" { ExtendedGlobKind::Question } /
            "*" { ExtendedGlobKind::Star }

        pub(crate) rule extended_glob_body() -> Vec<WordString> =
            first_branches:((b:extended_glob_branch() "|" { b })*) last_branch:extended_glob_branch() {
                let mut branches = first_branches;
                branches.push(last_branch);
                branches
            }

        rule extended_glob_branch() -> WordString =
            pieces:(!['|' | ')'] piece:pattern_piece() { piece })* { pieces.join("").into() }
    }
}

/// Returns whether or not a given character needs to be escaped in a regular expression.
///
/// # Arguments
///
/// * `c` - The character to check.
pub fn regex_char_needs_escaping(c: char) -> bool {
    matches!(
        c,
        '[' | ']' | '(' | ')' | '{' | '}' | '*' | '?' | '.' | '+' | '^' | '$' | '|' | '\\'
    )
}

fn escape_char_class_char_list(s: &WordString) -> WordString {
    s.replace('[', r"\[").into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_bracket_exprs() -> Result<()> {
        assert_eq!(pattern_to_regex_str(&"[a-z]".into(), true)?, "[a-z]");
        assert_eq!(pattern_to_regex_str(&"[abc]".into(), true)?, "[abc]");
        assert_eq!(pattern_to_regex_str(&r"[\(]".into(), true)?, r"[\(]");
        assert_eq!(pattern_to_regex_str(&r"[(]".into(), true)?, "[(]");
        assert_eq!(
            pattern_to_regex_str(&"[[:digit:]]".into(), true)?,
            "[[:digit:]]"
        );
        assert_eq!(
            pattern_to_regex_str(&r"[-(),!]*".into(), true)?,
            r"[-(),!].*"
        );
        assert_eq!(
            pattern_to_regex_str(&r"[-\(\),\!]*".into(), true)?,
            r"[-\(\),\!].*"
        );
        Ok(())
    }

    #[test]
    fn test_extended_glob() -> Result<()> {
        assert_eq!(
            pattern_to_regex_translator::extended_glob_pattern(&"@(a|b)".into(), true)?,
            "(a|b)"
        );

        assert_eq!(
            pattern_to_regex_translator::extended_glob_body(&"ab|ac".into(), true)?,
            vec!["ab", "ac"],
        );

        assert_eq!(
            pattern_to_regex_translator::extended_glob_pattern(&"*(ab|ac)".into(), true)?,
            "(ab|ac)*"
        );

        Ok(())
    }
}
