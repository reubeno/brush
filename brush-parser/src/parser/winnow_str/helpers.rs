use winnow::combinator::{peek, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::Offset;
use winnow::token::take_while;

use crate::ast::SeparatorOperator;

use super::types::{PError, StrStream};

// ============================================================================
// Tier 0: Character-level parsers (leaf functions)
// ============================================================================

/// Helper: Peek at next 1-2 operator characters for dispatch
pub(super) fn peek_op2<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    peek(winnow::token::take_while(1..=2, |c: char| {
        matches!(c, '<' | '>' | '&' | '|')
    }))
}

/// Helper: Peek at next 2-3 operator characters for case terminators
pub(super) fn peek_op3<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    peek(winnow::token::take_while(2..=3, |c: char| {
        matches!(c, ';' | '&')
    }))
}

/// Helper: Peek at first character for `word_part` dispatch
pub(super) fn peek_char<'a>() -> impl Parser<StrStream<'a>, char, PError> {
    peek(winnow::token::any)
}

/// Parse an extended glob pattern: @(...), +(...), *(...), ?(...), !(...)
/// Returns the entire pattern including the prefix and parentheses
pub(super) fn extglob_pattern<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    move |input: &mut StrStream<'a>| {
        // Save starting checkpoint to capture the prefix char too
        let start = input.checkpoint();

        // Match the prefix character (@, !, ?, +, *)
        let _prefix_char = winnow::token::one_of(['@', '!', '?', '+', '*']).parse_next(input)?;

        // Use the helper to parse balanced parens starting from the '('
        let _balanced =
            parse_balanced_delimiters("(", Some('('), ')', 1, false).parse_next(input)?;

        // Get the full pattern including prefix character
        let end = input.checkpoint();
        let consumed_len = end.offset_from(&start);

        input.reset(&start);
        let pattern = winnow::token::take(consumed_len).parse_next(input)?;

        Ok(pattern)
    }
}

// ============================================================================
// Helper: Quote Skipping Parsers
// ============================================================================

/// Skip the content of a single-quoted string, assuming the opening quote was already consumed.
/// Returns the content (without quotes) followed by the closing quote.
pub(super) fn skip_single_quoted_content<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    (take_while(0.., |c: char| c != '\''), '\'').take()
}

/// Skip the content of a double-quoted string, assuming the opening quote was already consumed.
/// Handles backslash escapes (\" and \\). Returns the content (without opening quote) followed by
/// the closing quote.
pub(super) fn skip_double_quoted_content<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        loop {
            match winnow::token::any::<_, PError>.parse_next(input) {
                Ok('"') => break,
                Ok('\\') => {
                    let _ = winnow::token::any::<_, PError>.parse_next(input);
                }
                Err(_) => {
                    return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
                }
                _ => {}
            }
        }

        let end = input.checkpoint();
        let consumed_len = end.offset_from(&start);
        input.reset(&start);
        winnow::token::take(consumed_len).parse_next(input)
    }
}

// ============================================================================
// Helper: Balanced Delimiter Parsing
// ============================================================================

/// Parse content with balanced delimiters (parentheses, braces, backticks)
/// Returns the full slice including opening and closing delimiters
///
/// # Parameters
/// - `prefix`: The opening delimiter(s) to match first (e.g., "$(", "${", backtick)
/// - `open_char`: Character that increases depth (e.g., '(' or '{'), or None for backticks
/// - `close_char`: Character that decreases depth (e.g., ')' or '}' or backtick)
/// - `initial_depth`: Starting depth (e.g., 1 for most, 2 for arithmetic `$((`)
/// - `allow_comments`: Whether to recognize `#` as starting a comment (true for command substitutions)
///
/// # Examples
/// - Command substitution: `parse_balanced_delimiters("$(", Some('('), ')', 1, true)`
/// - Arithmetic: `parse_balanced_delimiters("$((", Some('('), ')', 2, false)`
/// - Braced variable: `parse_balanced_delimiters("${", Some('{'), '}', 1, false)`
/// - Backtick: `parse_balanced_delimiters("`", None, '`', 1, true)`
pub(super) fn parse_balanced_delimiters<'a>(
    prefix: &'a str,
    open_char: Option<char>,
    close_char: char,
    initial_depth: usize,
    allow_comments: bool,
) -> impl Parser<StrStream<'a>, &'a str, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        // Match opening prefix - use winnow's literal parser
        winnow::token::literal(prefix).parse_next(input)?;

        // Parse balanced delimiters
        // Track whether # would start a comment (after whitespace/newline/start)
        // Only relevant when allow_comments is true
        let mut depth = initial_depth;
        let mut at_comment_start = allow_comments;

        while depth > 0 {
            match winnow::token::any::<_, PError>.parse_next(input) {
                Ok(ch) if Some(ch) == open_char => {
                    depth += 1;
                    at_comment_start = false;
                }
                Ok(ch) if ch == close_char => {
                    depth -= 1;
                    at_comment_start = false;
                }
                Ok('\\') => {
                    let _ = winnow::token::any::<_, PError>.parse_next(input);
                    at_comment_start = false;
                }
                Ok('\'') => {
                    let _ = skip_single_quoted_content().parse_next(input)?;
                    at_comment_start = false;
                }
                Ok('"') => {
                    let _ = skip_double_quoted_content().parse_next(input)?;
                    at_comment_start = false;
                }
                Ok('#') if at_comment_start => {
                    // Skip comment content (everything until newline, not consuming newline)
                    while let Ok(c) = winnow::token::any::<_, PError>.parse_next(input) {
                        if c == '\n' {
                            at_comment_start = true;
                            break;
                        }
                    }
                }
                Ok(ch) => {
                    at_comment_start = allow_comments && matches!(ch, ' ' | '\t' | '\n');
                }
                Err(_) => {
                    return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
                }
            }
        }

        // Get the full slice from start to current position
        let end = input.checkpoint();
        let consumed_len = end.offset_from(&start);

        input.reset(&start);
        let result = winnow::token::take(consumed_len).parse_next(input)?;

        Ok(result)
    }
}

/// Check if character is valid in a username for tilde expansion
/// POSIX portable filename characters: alphanumeric, dot, underscore, hyphen, plus
const fn is_username_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '_' | '-' | '+')
}

/// Parse a tilde expansion: ~, ~user, ~+, ~-, ~+N, ~-N
/// Returns the entire tilde expression as a string
pub(super) fn tilde_expansion<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    (
        '~',
        take_while(0.., is_username_char),
        peek(winnow::combinator::alt((
            winnow::combinator::eof.void(),
            winnow::token::one_of(['/', ':', ';', '}', ' ', '\t', '\n', '&', '|', '<', '>']).void(),
        ))),
    )
        .take()
}

/// Parse a newline character
/// Corresponds to: `matches_operator("\n`") in winnow.rs
#[inline]
pub(super) fn newline<'a>() -> impl Parser<StrStream<'a>, char, PError> {
    '\n'
}

/// Parse a comment: # to end of line (not including newline)
/// Comments start with # and continue to end of line
/// The # must appear at a word boundary (start of input or after whitespace)
#[inline]
pub(super) fn comment<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    ('#', take_while(0.., |c: char| c != '\n')).void()
}

/// Parse optional whitespace and comments (spaces, tabs, and comments, but NOT newlines)
///
/// Handles both inter-token spaces, inline comments, and backslash-newline
/// continuations like: `echo hello # comment` or `cmd \<NL> arg`.
/// This is needed to separate tokens on the same line.
#[inline]
pub(super) fn spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    repeat::<_, _, (), _, _>(
        0..,
        winnow::combinator::alt((
            take_while(1.., |c: char| c == ' ' || c == '\t').void(),
            ("\\", '\n').void(), // backslash-newline continuation
            comment(),
        )),
    )
    .void()
}

/// Parse required whitespace (at least one space or tab, optionally followed by comment)
#[inline]
pub(super) fn spaces1<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    (
        take_while(1.., |c: char| c == ' ' || c == '\t'), // Required spaces
        repeat::<_, _, (), _, _>(
            0..,
            winnow::combinator::alt((
                take_while(1.., |c: char| c == ' ' || c == '\t').void(),
                ("\\", '\n').void(), // backslash-newline continuation
            )),
        ),
        winnow::combinator::opt(comment()), // Optional comment after spaces
    )
        .void()
}

/// Parse whitespace inside array literals `( ... )`.
/// Newlines are treated as whitespace separators, just like spaces and tabs.
/// Also handles comments and backslash-newline continuations.
#[inline]
pub(super) fn array_spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    repeat::<_, _, (), _, _>(
        0..,
        winnow::combinator::alt((
            take_while(1.., |c: char| c == ' ' || c == '\t' || c == '\n').void(),
            ("\\", '\n').void(),
            comment(),
        )),
    )
    .void()
}

// ============================================================================
// Tier 1: Line breaks and separators
// ============================================================================

/// Parse linebreak (zero or more newlines, with optional comments before each newline)
/// Corresponds to: winnow.rs `linebreak()`
/// Handles blank lines, comment-only lines, and lines with inline comments
#[inline]
pub(super) fn linebreak<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    repeat::<_, _, (), _, _>(
        0..,
        (
            take_while(0.., |c: char| c == ' ' || c == '\t'), // Optional leading spaces
            winnow::combinator::opt(comment()),               // Optional comment
            newline(),                                        // Required newline
        )
            .void(),
    )
}

/// Parse newline list (one or more newlines, with optional comments before each newline)
/// Corresponds to: winnow.rs `newline_list()`
/// Handles blank lines, comment-only lines, and lines with inline comments
#[inline]
pub(super) fn newline_list<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    repeat::<_, _, (), _, _>(
        1..,
        (
            take_while(0.., |c: char| c == ' ' || c == '\t'), // Optional leading spaces
            winnow::combinator::opt(comment()),               // Optional comment
            newline(),                                        // Required newline
        )
            .void(),
    )
}

/// Parse separator operator (';' or '&')
/// Must NOT be part of a longer operator like ';;', ';&', '&&', etc.
/// Corresponds to: winnow.rs `separator_op()`
#[inline]
pub(super) fn separator_op<'a>() -> impl Parser<StrStream<'a>, SeparatorOperator, PError> {
    winnow::combinator::alt((
        // Match ';' but not if followed by another ';' or '&' (to avoid matching ";;" or ";&")
        winnow::combinator::terminated(
            ';',
            winnow::combinator::peek(winnow::combinator::not(winnow::token::one_of([';', '&']))),
        )
        .value(SeparatorOperator::Sequence),
        // Match '&' but not if followed by another '&' (to avoid matching "&&")
        winnow::combinator::terminated('&', winnow::combinator::peek(winnow::combinator::not('&')))
            .value(SeparatorOperator::Async),
    ))
}

/// Parse separator (`separator_op` with linebreak, or `newline_list`)
/// Returns Option<SeparatorOperator> - None means it was just newlines
/// Corresponds to: winnow.rs `separator()` and peg.rs `separator()`
#[inline]
pub(super) fn separator<'a>() -> impl Parser<StrStream<'a>, Option<SeparatorOperator>, PError> {
    winnow::combinator::alt((
        // separator_op followed by optional linebreaks
        (separator_op(), linebreak()).map(|(sep, ())| Some(sep)),
        // OR just one or more newlines (acts as sequence separator)
        newline_list().map(|()| None),
    ))
}

/// Parse a sequential separator (semicolon or newlines)
/// Corresponds to: winnow.rs `sequential_sep()`
#[inline]
pub(super) fn sequential_sep<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    winnow::combinator::alt(((spaces(), ';', linebreak()).void(), newline_list().void()))
}

/// Match a specific keyword (shell reserved word)
/// Keywords must be followed by a delimiter (space, tab, newline, semicolon, etc.)
/// to avoid matching them as part of a larger word
pub(super) fn keyword<'a>(word: &'static str) -> impl Parser<StrStream<'a>, &'a str, PError> {
    winnow::combinator::preceded(
        spaces(),
        winnow::token::literal(word).verify(|_: &str| true), // Will be followed by delimiter check
    )
    .context(winnow::error::StrContext::Label("keyword"))
    .verify(move |_: &str| {
        // After matching the keyword, check that it's followed by a delimiter
        // We can't easily peek ahead in winnow without consuming, so we rely on
        // the fact that bare_word won't match delimiters
        true // TODO: Add proper word boundary check
    })
}

/// Peek the first word without consuming input (for keyword dispatch)
pub(super) fn peek_first_word<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    winnow::combinator::peek(take_while(1.., |c: char| c.is_alphanumeric() || c == '_'))
}

/// Check if a string is a valid shell variable name
/// Names must start with [a-zA-Z_] and contain only [a-zA-Z0-9_]
pub(super) fn is_valid_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if a string is a valid bash function name.
///
/// Bash function names may contain any characters that are valid in a word
/// (including hyphens, dots, and digits at the start), unlike variable names
/// which are restricted to `[a-zA-Z_][a-zA-Z0-9_]*`. The name must not end
/// with `=` (to avoid ambiguity with assignments) and must not be a single
/// shell metacharacter like `{` or `}`.
pub(super) fn is_valid_fname(s: &str) -> bool {
    if s.is_empty() || s.ends_with('=') {
        return false;
    }
    // Reject single metacharacters that bare_word might match
    if s == "{" || s == "}" {
        return false;
    }
    true
}

/// Parse a valid variable name
/// Corresponds to: winnow.rs `name()`
pub(super) fn name<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::preceded(spaces(), super::words::bare_word())
        .verify(|s: &str| is_valid_name(s))
        .map(|s: &str| s.to_string())
}

/// Parse a function name.
pub(super) fn fname<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::preceded(spaces(), super::words::bare_word())
        .verify(|s: &str| is_valid_fname(s))
        .map(|s: &str| s.to_string())
}

/// Check if a string is a shell reserved word
///
/// Note: This list matches the PEG parser's reserved word list.
/// "time" and "coproc" are bash reserved words but are NOT included here
/// to match PEG parser behavior (which allows them as command names).
pub(super) fn is_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "do"
            | "done"
            | "while"
            | "until"
            | "for"
            | "in"
            | "case"
            | "esac"
            | "function"
            | "{"
            | "}"
            | "!"
            | "[["
            | "]]"
            | "select"
    )
}
