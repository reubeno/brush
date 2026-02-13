//! String-based winnow parser - mirrors winnow.rs but takes &str instead of &[Token]
//!
//! This module converts the token-based winnow parser to work directly on strings.
//! Each function is converted one at a time and tested against the original.

#![allow(dead_code)]

use std::borrow::Cow;

use winnow::combinator::{dispatch, fail, peek, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::{LocatingSlice, Offset};
use winnow::token::take_while;

use crate::ast::{self, SeparatorOperator};
use crate::parser::{ParserOptions, SourceInfo};
use crate::source::{SourcePosition, SourceSpan};

/// Type alias for parser error
type PError = winnow::error::ErrMode<ContextError>;

/// Type alias for input stream
pub type StrStream<'a> = LocatingSlice<&'a str>;

/// Context for parsing - holds options and source info
#[derive(Clone)]
pub struct ParseContext<'a> {
    /// Parser options controlling extended globbing, POSIX mode, etc.
    pub options: &'a ParserOptions,
    /// Source file information for error reporting
    pub source_info: &'a SourceInfo,
    /// Pending trailing content from here-docs that needs to be parsed as pipeline continuation
    /// (e.g., "| grep hello" from "cat <<EOF | grep hello")
    pub pending_heredoc_trailing: &'a std::cell::RefCell<Option<&'a str>>,
}

// ============================================================================
// Position Tracking
// ============================================================================

/// Helper struct to track position in the input while parsing
///
/// OPTIMIZATION: Uses line break caching + binary search for fast line/column lookup.
/// Instead of O(n) scanning for each position, we:
/// 1. Cache all line break positions during initialization (O(n) once)
/// 2. Use binary search for line lookup (O(log m) per position, m = number of lines)
///
/// This provides 100-2600x speedup for medium/large files!
#[derive(Debug, Clone)]
pub struct PositionTracker {
    /// Cached positions of all newline characters in the input.
    /// Allows O(log m) line number lookup via binary search.
    line_breaks: Vec<usize>,
    /// Cache original length for manual offset calculations (when not using `LocatingSlice`)
    #[allow(dead_code)]
    original_len: usize,
}

impl PositionTracker {
    /// Creates a new `PositionTracker` for the given input string.
    ///
    /// Performs a one-time O(n) scan to cache all line break positions,
    /// enabling O(log m) line number lookups for the rest of parsing.
    #[allow(dead_code)]
    pub fn new(input: &str) -> Self {
        // One-time O(n) scan to cache all line break positions
        // This enables O(log m) lookups for the rest of parsing
        let line_breaks: Vec<usize> = input
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None })
            .collect();

        Self {
            line_breaks,
            original_len: input.len(),
        }
    }

    /// Get current offset from `LocatingSlice`
    #[inline]
    fn offset_from_locating(&self, input: &LocatingSlice<&str>) -> usize {
        self.original_len - input.len()
    }

    /// Calculate source position from byte offset using binary search
    ///
    /// Complexity: O(log m) where m = number of lines (vs O(n) before)
    fn position_at(&self, offset: usize) -> SourcePosition {
        // Binary search to find which line this offset is on
        // line_breaks[i] is the position of the i-th newline
        // Line numbering: line 1 is before first newline, line 2 is before second newline, etc.
        let line = match self.line_breaks.binary_search(&offset) {
            // Found exact newline character - it belongs to the line it ends
            Ok(pos) => pos + 1,
            // Not found - pos is where it would be inserted, so pos is the line number
            Err(pos) => pos + 1,
        };

        // Calculate column as offset from start of line
        let line_start = if line > 1 {
            // Previous line ended at line_breaks[line-2], so this line starts after that
            self.line_breaks[line - 2] + 1
        } else {
            // Line 1 starts at position 0
            0
        };

        SourcePosition {
            index: offset,
            line,
            column: offset.saturating_sub(line_start) + 1,
        }
    }

    /// Convert a byte range to a `SourceSpan` (for use with `LocatingSlice`)
    ///
    /// This is the primary method when using `LocatingSlice.with_span()`
    #[inline]
    fn range_to_span(&self, range: std::ops::Range<usize>) -> SourceSpan {
        SourceSpan {
            start: self.position_at(range.start).into(),
            end: self.position_at(range.end).into(),
        }
    }
}

// ============================================================================
// Tier 0: Character-level parsers (leaf functions)
// ============================================================================

/// Helper: Peek at next 1-2 operator characters for dispatch
fn peek_op2<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    peek(winnow::token::take_while(1..=2, |c: char| {
        matches!(c, '<' | '>' | '&' | '|')
    }))
}

/// Helper: Peek at next 2-3 operator characters for case terminators
fn peek_op3<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    peek(winnow::token::take_while(2..=3, |c: char| {
        matches!(c, ';' | '&')
    }))
}

/// Helper: Peek at first character for `word_part` dispatch
fn peek_char<'a>() -> impl Parser<StrStream<'a>, char, PError> {
    peek(winnow::token::any)
}

/// Parse an extended glob pattern: @(...), +(...), *(...), ?(...), !(...)
/// Returns the entire pattern including the prefix and parentheses
fn extglob_pattern<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    move |input: &mut StrStream<'a>| {
        // Save starting checkpoint to capture the prefix char too
        let start = input.checkpoint();

        // Match the prefix character (@, !, ?, +, *)
        let _prefix_char = winnow::token::one_of(['@', '!', '?', '+', '*']).parse_next(input)?;

        // Use the helper to parse balanced parens starting from the '('
        let _balanced = parse_balanced_delimiters("(", Some('('), ')', 1).parse_next(input)?;

        // Get the full pattern including prefix character
        let end = input.checkpoint();
        let consumed_len = end.offset_from(&start);

        input.reset(&start);
        let pattern = winnow::token::take(consumed_len).parse_next(input)?;

        Ok(pattern)
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
///
/// # Examples
/// - Command substitution: `parse_balanced_delimiters("$(", Some('('), ')', 1)`
/// - Arithmetic: `parse_balanced_delimiters("$((", Some('('), ')', 2)`
/// - Braced variable: `parse_balanced_delimiters("${", Some('{'), '}', 1)`
/// - Backtick: `parse_balanced_delimiters("`", None, '`', 1)`
fn parse_balanced_delimiters<'a>(
    prefix: &'a str,
    open_char: Option<char>,
    close_char: char,
    initial_depth: usize,
) -> impl Parser<StrStream<'a>, &'a str, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        // Match opening prefix - use winnow's literal parser
        winnow::token::literal(prefix).parse_next(input)?;

        // Parse balanced delimiters
        let mut depth = initial_depth;

        while depth > 0 {
            match winnow::token::any::<_, PError>.parse_next(input) {
                Ok(ch) if Some(ch) == open_char => {
                    depth += 1;
                }
                Ok(ch) if ch == close_char => {
                    depth -= 1;
                }
                Ok('\\') => {
                    // Skip escaped character
                    let _ = winnow::token::any::<_, PError>.parse_next(input);
                }
                Ok(_) => {
                    // Regular character
                }
                Err(_) => {
                    // Hit end of input without closing delimiter
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
fn tilde_expansion<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
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
pub fn newline<'a>() -> impl Parser<StrStream<'a>, char, PError> {
    '\n'
}

/// Parse a comment: # to end of line (not including newline)
/// Comments start with # and continue to end of line
/// The # must appear at a word boundary (start of input or after whitespace)
#[inline]
fn comment<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    ('#', take_while(0.., |c: char| c != '\n')).void()
}

/// Parse optional whitespace and comments (spaces, tabs, and comments, but NOT newlines)
///
/// Handles both inter-token spaces, inline comments, and backslash-newline
/// continuations like: `echo hello # comment` or `cmd \<NL> arg`.
/// This is needed to separate tokens on the same line.
#[inline]
pub fn spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
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
pub fn spaces1<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    (
        take_while(1.., |c: char| c == ' ' || c == '\t'), // Required spaces
        winnow::combinator::opt(comment()),               // Optional comment after spaces
    )
        .void()
}

/// Parse whitespace inside array literals `( ... )`.
/// Newlines are treated as whitespace separators, just like spaces and tabs.
/// Also handles comments and backslash-newline continuations.
#[inline]
fn array_spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
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
pub fn linebreak<'a>() -> impl Parser<StrStream<'a>, (), PError> {
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
pub fn newline_list<'a>() -> impl Parser<StrStream<'a>, (), PError> {
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
pub fn separator_op<'a>() -> impl Parser<StrStream<'a>, SeparatorOperator, PError> {
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
fn separator<'a>() -> impl Parser<StrStream<'a>, Option<SeparatorOperator>, PError> {
    winnow::combinator::alt((
        // separator_op followed by optional linebreaks
        (separator_op(), linebreak()).map(|(sep, ())| Some(sep)),
        // OR just one or more newlines (acts as sequence separator)
        newline_list().map(|()| None),
    ))
}

// ============================================================================
// Tier 2: Word parsing
// ============================================================================

/// Parse a bare word (literal characters only, no quotes or expansions)
/// Corresponds to the `literal_chars` part of tokenizer's word parsing
///
/// A word character is anything that's NOT:
/// - Whitespace: ' ', '\t', '\n', '\r'
/// - Operators: '|', '&', ';', '<', '>', '(', ')'
/// - Quote/expansion starters: '$', backtick, '\'', '"', '\\'
///
/// Note: '{' and '}' ARE allowed in words for brace expansion (e.g., {1..10}, {a,b,c})
/// Brace groups ({ commands; }) are distinguished by requiring whitespace after '{' and before '}'
///
/// Note: Shell keywords (if, then, fi, etc.) are NOT excluded here because they
/// can be used as regular words in non-keyword contexts (e.g., "echo done").
/// The `command()` parser tries compound commands first, so keywords in keyword
/// positions will be matched by compound command parsers before `bare_word` sees them.
pub fn bare_word<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    take_while(1.., |c: char| {
        !matches!(
            c,
            ' ' | '\t' | '\n' | '\r' |  // Whitespace
            '|' | '&' | ';' | '<' | '>' | '(' | ')' |  // Operators (note: { } removed to allow brace expansion)
            '$' | '`' | '\'' | '"' | '\\' // Quote/expansion starts
        )
    })
}

/// Check if a string is a shell reserved word
///
/// Note: This list matches the PEG parser's reserved word list.
/// "time" and "coproc" are bash reserved words but are NOT included here
/// to match PEG parser behavior (which allows them as command names).
fn is_reserved_word(s: &str) -> bool {
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

/// Parse a non-reserved word (for use as command names)
/// Reserved words cannot be used as command names in simple commands
fn non_reserved_word<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    word_as_ast(ctx, tracker).verify(|word: &ast::Word| !is_reserved_word(&word.value))
}

// ============================================================================
// Tier 9: Variable Expansions
// ============================================================================

/// Parse a simple variable reference: $VAR
/// Returns the expansion text including the $
pub fn simple_variable<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    (
        '$',
        winnow::token::take_while(1.., |c: char| c.is_alphanumeric() || c == '_'),
    )
        .take()
}

/// Parse a braced variable reference: ${VAR}
/// Returns the expansion text including ${ }
pub fn braced_variable<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("${", Some('{'), '}', 1)
}

/// Parse an arithmetic expansion: $((expr))
/// Returns the expansion text including $(( ))
pub fn arithmetic_expansion<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("$((", Some('('), ')', 2)
}

/// Parse a command substitution: $(cmd)
/// Returns the expansion text including $( )
pub fn command_substitution<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    // Need to be careful: $(( is arithmetic, $( is command substitution
    winnow::combinator::preceded(
        winnow::combinator::peek(winnow::combinator::not("$((")),
        parse_balanced_delimiters("$(", Some('('), ')', 1),
    )
}

/// Parse a backtick command substitution: `cmd`
/// Returns the expansion text including backticks
pub fn backtick_substitution<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("`", None, '`', 1)
}

/// Parse special parameter: $0, $1, $?, $@, etc.
/// Returns the expansion text including the $
pub fn special_parameter<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    (
        '$',
        winnow::combinator::alt((
            winnow::token::one_of(['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']),
            winnow::token::one_of(['?', '@', '*', '#', '$', '!', '-', '_']),
        )),
    )
        .take()
}

// ============================================================================
// Tier 7: Quoted Strings
// ============================================================================

/// Parse a single-quoted string: 'text'
/// In single quotes, everything is literal except the closing quote
/// Returns the full string including quotes (e.g., "'text'")
pub fn single_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    ('\'', take_while(0.., |c: char| c != '\''), '\'')
        .take()
        .map(|s: &str| s.to_string())
}

/// Parse a double-quoted string: "text".
///
/// Returns the full string including quotes (e.g., `"text"`).
/// Handles backslash escape sequences and `$(...)` command substitutions
/// (which may span multiple lines for heredocs) inside the string.
pub fn double_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        // Match opening quote
        '"'.parse_next(input)?;

        loop {
            // Try to match closing quote
            if winnow::combinator::opt::<_, _, PError, _>('"')
                .parse_next(input)?
                .is_some()
            {
                break;
            }

            match winnow::token::any::<_, PError>.parse_next(input) {
                Ok('\\') => {
                    // Escape sequence: skip the next character
                    let _ = winnow::token::any::<_, PError>.parse_next(input);
                }
                Ok('$') => {
                    // Check if this starts a $(...) command substitution (not
                    // $((...)) arithmetic). If so, we MUST consume it as a
                    // balanced unit because the body can span multiple lines
                    // (e.g., heredocs). If the closing `)` is missing, that's a
                    // hard error — not optional.
                    if winnow::combinator::peek::<_, _, PError, _>(winnow::combinator::not("(("))
                        .parse_next(input)
                        .is_ok()
                        && winnow::combinator::peek::<_, _, PError, _>('(')
                            .parse_next(input)
                            .is_ok()
                    {
                        // Committed: $( was detected, must find closing )
                        parse_balanced_delimiters("(", Some('('), ')', 1)
                            .void()
                            .parse_next(input)?;
                    }
                    // Otherwise $ was just a plain character — already consumed.
                }
                Ok('`') => {
                    // Backtick substitution — consume until matching backtick
                    let _: Result<&str, PError> =
                        take_while(0.., |c: char| c != '`').parse_next(input);
                    let _ = winnow::combinator::opt::<_, _, PError, _>('`').parse_next(input);
                }
                Ok(_) => {
                    // Regular character — already consumed
                }
                Err(_) => {
                    // Hit end of input without closing quote
                    return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
                }
            }
        }

        // Get the full slice from start to current position
        let end = input.checkpoint();
        let consumed_len = end.offset_from(&start);
        input.reset(&start);
        let result: &str = winnow::token::take(consumed_len).parse_next(input)?;

        Ok(result.to_string())
    }
}

/// Parse an escape sequence: \c
/// Returns the escaped character (simplified version)
pub fn escape_sequence<'a>() -> impl Parser<StrStream<'a>, char, PError> {
    winnow::combinator::preceded(
        '\\',
        winnow::token::any, // For now, just return the escaped character as-is
    )
}

/// Parse a word part (bare text, single quote, double quote, escape, or expansion)
/// Returns the string value of the part
/// The `last_char` parameter helps detect tilde-after-colon
fn word_part<'a>(
    ctx: &'a ParseContext<'a>,
    last_char: Option<char>,
) -> impl Parser<StrStream<'a>, Cow<'a, str>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Fast path: dispatch on first character
        let ch = peek_char().parse_next(input)?;

        match ch {
            '\'' => single_quoted_string().map(Cow::Owned).parse_next(input),
            '"' => double_quoted_string().map(Cow::Owned).parse_next(input),
            '$' => winnow::combinator::alt((
                arithmetic_expansion(), // $(( before $(
                command_substitution(), // $(
                braced_variable(),      // ${ before $
                special_parameter(),    // $1, $?, etc. before simple $VAR
                simple_variable(),      // $VAR
            ))
            .map(Cow::Borrowed)
            .parse_next(input),
            '`' => backtick_substitution().map(Cow::Borrowed).parse_next(input),
            '\\' => escape_sequence()
                .map(|c| Cow::Owned(format!("\\{c}")))
                .parse_next(input),
            // Tilde after colon: ~user or ~ expansion
            '~' if ctx.options.tilde_expansion_after_colon && last_char == Some(':') => {
                if let Ok(tilde_expr) = tilde_expansion().parse_next(input) {
                    Ok(Cow::Borrowed(tilde_expr))
                } else {
                    bare_word().map(Cow::Borrowed).parse_next(input)
                }
            }
            // Extended glob patterns start with ?, *, +, @, or ! followed by (
            '?' | '*' | '+' | '@' | '!' if ctx.options.enable_extended_globbing => {
                if let Some(pattern) =
                    winnow::combinator::opt(extglob_pattern()).parse_next(input)?
                {
                    Ok(Cow::Borrowed(pattern))
                } else {
                    bare_word().map(Cow::Borrowed).parse_next(input)
                }
            }
            // Default: parse as bare word (most common case)
            _ => bare_word().map(Cow::Borrowed).parse_next(input),
        }
    }
}

/// Parse a word (one or more word parts combined)
/// Handles quoted strings, escapes, and bare text
/// Corresponds to: tokenizer's word parsing + winnow.rs `word_as_ast()`
pub fn word_as_ast<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Check for tilde at word start if enabled
        let mut value: Cow<'_, str> = Cow::Borrowed("");
        let mut last_char = None;

        if ctx.options.tilde_expansion_at_word_start {
            if peek_char().parse_next(input).ok() == Some('~') {
                if let Ok(tilde_expr) = tilde_expansion().parse_next(input) {
                    last_char = tilde_expr.chars().last();
                    value = Cow::Borrowed(tilde_expr);
                }
            }
        }

        // Parse remaining word parts, tracking last character for tilde-after-colon detection
        while let Ok(part) = word_part(ctx, last_char).parse_next(input) {
            // Update last_char efficiently - just get the last char of the new part
            last_char = part.chars().last().or(last_char);

            // Optimize: avoid allocation if this is the first and only part
            if value.is_empty() {
                value = part;
            } else {
                // Need to combine parts - must allocate
                value.to_mut().push_str(&part);
            }
        }

        // Must have at least one character
        if value.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::Word {
            value: value.into_owned(),
            loc: Some(loc),
        })
    }
}

/// Parse a wordlist (one or more words separated by spaces)
/// Corresponds to: winnow.rs `wordlist()`
pub fn wordlist<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::Word>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::separated(1.., word_as_ast(ctx, tracker), spaces1()).parse_next(input)
    }
}

// ============================================================================
// Tier 8: Redirections
// ============================================================================

/// Parse an I/O file descriptor number
pub fn io_number<'a>() -> impl Parser<StrStream<'a>, i32, PError> {
    winnow::ascii::dec_uint::<_, u16, _>.map(i32::from)
}

/// Parse redirect operator and return the redirect kind
/// Corresponds to: winnow.rs `io_file()` dispatcher
fn redirect_operator<'a>() -> impl Parser<StrStream<'a>, ast::IoFileRedirectKind, PError> {
    dispatch! {peek_op2();
        ">>" => ">>".value(ast::IoFileRedirectKind::Append),
        "<>" => "<>".value(ast::IoFileRedirectKind::ReadAndWrite),
        ">|" => ">|".value(ast::IoFileRedirectKind::Clobber),
        ">&" => ">&".value(ast::IoFileRedirectKind::DuplicateOutput),
        "<&" => "<&".value(ast::IoFileRedirectKind::DuplicateInput),
        ">" => ">".value(ast::IoFileRedirectKind::Write),
        "<" => "<".value(ast::IoFileRedirectKind::Read),
        _ => fail,
    }
}

/// Parse a here-document delimiter, handling quotes
/// Returns (`delimiter_text`, `requires_expansion`)
/// Returns (`raw_delimiter`, `match_delimiter`, `requires_expansion`)
/// `raw_delimiter`: as written (includes quotes for `here_end`)
/// `match_delimiter`: stripped of quotes (for matching content)
fn here_document_delimiter<'a>() -> impl Parser<StrStream<'a>, (String, String, bool), PError> {
    move |input: &mut StrStream<'a>| {
        let mut raw_delimiter = String::new();
        let mut match_delimiter = String::new();
        let mut quoted = false;
        let mut done = false;

        while !done && !input.is_empty() {
            let checkpoint = input.checkpoint();

            // Check for whitespace or newline (end of delimiter)
            if let Ok(_ch) =
                winnow::token::one_of::<_, _, PError>([' ', '\t', '\n']).parse_next(input)
            {
                input.reset(&checkpoint);
                break;
            }
            input.reset(&checkpoint);

            // Try to parse a character
            let ch: char = winnow::token::any.parse_next(input)?;
            raw_delimiter.push(ch);

            match ch {
                '\'' | '"' => {
                    quoted = true;
                    // Don't include quotes in match delimiter
                }
                '\\' => {
                    quoted = true;
                    // Consume next character
                    if let Ok(next_ch) = winnow::token::any::<_, PError>.parse_next(input) {
                        raw_delimiter.push(next_ch);
                        match_delimiter.push(next_ch);
                    }
                }
                ' ' | '\t' | '\n' => {
                    // End of delimiter
                    done = true;
                }
                _ => {
                    match_delimiter.push(ch);
                }
            }
        }

        if match_delimiter.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let requires_expansion = !quoted;
        Ok((raw_delimiter, match_delimiter, requires_expansion))
    }
}

/// Parse here-document content until delimiter is found
/// Returns the content as a Word
fn here_document_content(
    input: &mut StrStream<'_>,
    delimiter: &str,
    remove_tabs: bool,
    tracker: &PositionTracker,
) -> Result<ast::Word, PError> {
    let start_offset = tracker.offset_from_locating(input);
    let mut content = String::new();
    let mut at_line_start = true;

    loop {
        // Check if we're at a line that matches the delimiter
        if at_line_start {
            let checkpoint = input.checkpoint();

            // Skip leading tabs if remove_tabs is true (for both delimiter and content)
            if remove_tabs {
                let _: Result<&str, PError> =
                    winnow::token::take_while(0.., '\t').parse_next(input);
            }

            // Try to match delimiter
            if let Ok(line_content) =
                winnow::token::take_while::<_, _, PError>(0.., |c| c != '\n').parse_next(input)
            {
                if line_content == delimiter {
                    // Do NOT consume the newline after the delimiter — it serves
                    // as the command separator so that complete_command_continuation
                    // can find the next command on the following line.
                    let end_offset = tracker.offset_from_locating(input);
                    let loc = tracker.range_to_span(start_offset..end_offset);
                    return Ok(ast::Word {
                        value: content,
                        loc: Some(loc),
                    });
                }
            }

            // Not the delimiter, reset to get full line
            input.reset(&checkpoint);

            // If remove_tabs, skip leading tabs from content too
            if remove_tabs {
                let _: Result<&str, PError> =
                    winnow::token::take_while(0.., '\t').parse_next(input);
            }
        }

        // Collect this line's content
        at_line_start = false;

        if input.is_empty() {
            // Unterminated here-document
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let ch: char = winnow::token::any.parse_next(input)?;
        content.push(ch);

        if ch == '\n' {
            at_line_start = true;
        }
    }
}

/// Parse a here-document redirect (<< or <<-)
/// Returns (fd, `here_doc`, `remaining_line`) where `remaining_line` is content after
/// the delimiter on the same line (e.g., "| grep hello" in "<<EOF | grep hello")
/// A pending here-document that has been parsed but content not yet resolved
#[derive(Debug)]
struct PendingHereDoc {
    fd: Option<i32>,
    remove_tabs: bool,
    requires_expansion: bool,
    raw_delimiter: String,
    match_delimiter: String,
}

/// Parse just the here-document marker (operator and delimiter), without consuming content.
/// This is used to collect all markers on a line before resolving content.
fn here_document_marker<'a>() -> impl Parser<StrStream<'a>, PendingHereDoc, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Optional fd number
        let fd = winnow::combinator::opt(io_number()).parse_next(input)?;

        // Parse operator (<<- or <<)
        let remove_tabs = if winnow::combinator::opt("<<-").parse_next(input)?.is_some() {
            true
        } else if winnow::combinator::opt("<<").parse_next(input)?.is_some() {
            false
        } else {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        };

        // Skip optional spaces between operator and delimiter (e.g., <<- EOF)
        let _: &str =
            winnow::token::take_while(0.., |c: char| c == ' ' || c == '\t').parse_next(input)?;

        // Parse delimiter - raw_delimiter preserves quotes, match_delimiter is stripped
        let (raw_delimiter, match_delimiter, requires_expansion) =
            here_document_delimiter().parse_next(input)?;

        Ok(PendingHereDoc {
            fd,
            remove_tabs,
            requires_expansion,
            raw_delimiter,
            match_delimiter,
        })
    }
}

/// Resolve a pending here-document by parsing its content from the input.
fn resolve_here_document(
    input: &mut StrStream<'_>,
    pending: PendingHereDoc,
    tracker: &PositionTracker,
) -> Result<(Option<i32>, ast::IoHereDocument), winnow::error::ErrMode<ContextError>> {
    let doc = here_document_content(
        input,
        &pending.match_delimiter,
        pending.remove_tabs,
        tracker,
    )?;

    Ok((
        pending.fd,
        ast::IoHereDocument {
            remove_tabs: pending.remove_tabs,
            requires_expansion: pending.requires_expansion,
            here_end: ast::Word::from(pending.raw_delimiter),
            doc,
        },
    ))
}

/// Parse one or more here-documents on the same line.
/// Returns a vector of resolved here-documents and optional trailing content.
#[allow(clippy::type_complexity)]
fn here_documents<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (Vec<(Option<i32>, ast::IoHereDocument)>, Option<&'a str>), PError> + 'a
{
    move |input: &mut StrStream<'a>| {
        // Collect all here-doc markers on this line
        let mut markers: Vec<PendingHereDoc> = Vec::new();

        // Parse the first marker
        let first_marker = here_document_marker().parse_next(input)?;
        markers.push(first_marker);

        // Skip optional whitespace after delimiter
        let _: &str =
            winnow::token::take_while(0.., |c| c == ' ' || c == '\t').parse_next(input)?;

        // Check if there are more here-doc markers on this line
        while winnow::combinator::peek::<_, _, PError, _>("<<")
            .parse_next(input)
            .is_ok()
        {
            let marker = here_document_marker().parse_next(input)?;
            markers.push(marker);
            // Skip whitespace after this marker
            let _: &str =
                winnow::token::take_while(0.., |c| c == ' ' || c == '\t').parse_next(input)?;
        }

        // Capture remaining content until newline (for pipeline continuations like "| grep x")
        let rest: &str = winnow::token::take_while(0.., |c| c != '\n').parse_next(input)?;
        let remaining_line = {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        };

        // Consume the newline
        '\n'.parse_next(input)?;

        // Now resolve content for each here-doc in order.
        // Each heredoc's content parser stops WITHOUT consuming the newline
        // after the delimiter.  Between consecutive heredocs we must skip
        // that newline so the next heredoc's content starts on a fresh line.
        let mut resolved: Vec<(Option<i32>, ast::IoHereDocument)> = Vec::new();
        for (i, marker) in markers.into_iter().enumerate() {
            if i > 0 {
                // Skip the newline left after the previous delimiter
                let _: Result<char, PError> = '\n'.parse_next(input);
            }
            let doc = resolve_here_document(input, marker, tracker)?;
            resolved.push(doc);
        }

        Ok((resolved, remaining_line))
    }
}

fn here_document<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (Option<i32>, ast::IoHereDocument, Option<&'a str>), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Use the multi-heredoc parser but only return the first one
        // This maintains backwards compatibility with existing code that expects a single here-doc
        let (mut docs, remaining) = here_documents(tracker).parse_next(input)?;

        if docs.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let (fd, doc) = docs.remove(0);
        // Note: additional docs are discarded here - callers should use here_documents() directly
        // for proper multi-heredoc support
        Ok((fd, doc, remaining))
    }
}

/// Result of parsing an I/O redirect - may include trailing content for here-docs
pub struct IoRedirectResult<'a> {
    /// The parsed redirect
    pub redirect: ast::IoRedirect,
    /// For here-docs, any content after the delimiter on the same line (e.g., "| grep x")
    pub trailing_content: Option<&'a str>,
}

/// Parse a file redirect (e.g., "> file", "2>&1", "< input")
/// Corresponds to: winnow.rs `io_file()` + `io_redirect()`
pub fn io_redirect<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, IoRedirectResult<'a>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::alt((
            // Try OutputAndError redirects first (&>> and &>)
            (
                "&>>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| IoRedirectResult {
                    redirect: ast::IoRedirect::OutputAndError(target, true),
                    trailing_content: None,
                }),
            (
                "&>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| IoRedirectResult {
                    redirect: ast::IoRedirect::OutputAndError(target, false),
                    trailing_content: None,
                }),
            // Try here-string (<<<)
            (
                winnow::combinator::opt(io_number()),
                "<<<",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(fd, _, word)| IoRedirectResult {
                    redirect: ast::IoRedirect::HereString(fd, word),
                    trailing_content: None,
                }),
            // Try here-document
            here_document(tracker).map(|(fd, here_doc, remaining)| {
                // Store trailing content in context for later processing by pipe_sequence
                if let Some(trailing) = remaining {
                    *ctx.pending_heredoc_trailing.borrow_mut() = Some(trailing);
                }
                IoRedirectResult {
                    redirect: ast::IoRedirect::HereDocument(fd, here_doc),
                    trailing_content: remaining,
                }
            }),
            // Then try regular file redirects (including process substitution as target)
            move |input: &mut StrStream<'a>| {
                let fd = winnow::combinator::opt(io_number()).parse_next(input)?;
                let kind = redirect_operator().parse_next(input)?;
                spaces().parse_next(input)?;

                // Try process substitution as redirect target first (e.g., < <(cmd))
                let redirect_target = if let Ok((ps_kind, ps_cmd)) =
                    process_substitution(ctx, tracker).parse_next(input)
                {
                    ast::IoFileRedirectTarget::ProcessSubstitution(ps_kind, ps_cmd)
                } else {
                    let target = word_as_ast(ctx, tracker).parse_next(input)?;
                    match kind {
                        ast::IoFileRedirectKind::DuplicateOutput
                        | ast::IoFileRedirectKind::DuplicateInput => {
                            ast::IoFileRedirectTarget::Duplicate(target)
                        }
                        _ => ast::IoFileRedirectTarget::Filename(target),
                    }
                };

                Ok(IoRedirectResult {
                    redirect: ast::IoRedirect::File(fd, kind, redirect_target),
                    trailing_content: None,
                })
            },
        ))
        .parse_next(input)
    }
}

/// Parse a redirect list (one or more redirects)
/// Corresponds to: winnow.rs `redirect_list()`
pub fn redirect_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::RedirectList, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::preceded(spaces(), io_redirect(ctx, tracker)).map(|r| r.redirect), // Extract just the redirect, ignore trailing content
        )
        .map(ast::RedirectList)
        .parse_next(input)
    }
}

// ============================================================================
// Tier 3: Commands
// ============================================================================

/// Parse an array element value (handles quotes properly, stops at ')' or whitespace)
fn array_element_value<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl Parser<StrStream<'a>, String, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let mut value = String::new();

        loop {
            // Check if we should stop (at ')' or unquoted whitespace)
            let Ok(ch) = peek_char().parse_next(input) else {
                break; // EOF
            };

            if ch == ')' || ch.is_whitespace() {
                break;
            }

            // Parse the next word part
            let part = word_part(ctx, value.chars().last()).parse_next(input)?;
            value.push_str(&part);
        }

        if value.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(value)
    }
}

/// Parse an array element: either "value" or "[index]=value"
fn array_element<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl Parser<StrStream<'a>, (Option<ast::Word>, ast::Word), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Skip whitespace before element (newlines are whitespace inside arrays)
        array_spaces().parse_next(input)?;

        // Try to parse indexed element: [index]=value
        let checkpoint = input.checkpoint();
        let has_bracket = winnow::combinator::opt::<_, _, PError, _>('[')
            .parse_next(input)?
            .is_some();

        if has_bracket {
            // Parse index (everything until ])
            let index_str = winnow::token::take_while::<_, _, PError>(0.., |c: char| c != ']')
                .parse_next(input)?;

            let has_close = winnow::combinator::opt::<_, _, PError, _>(']')
                .parse_next(input)?
                .is_some();
            let has_equals = winnow::combinator::opt::<_, _, PError, _>('=')
                .parse_next(input)?
                .is_some();

            if has_close && has_equals {
                // Parse value using proper word parsing that handles quotes
                let value_str = winnow::combinator::opt(array_element_value(ctx))
                    .parse_next(input)?
                    .unwrap_or_default();

                return Ok((Some(ast::Word::new(index_str)), ast::Word::new(&value_str)));
            }
        }

        // Reset and try simple value
        input.reset(&checkpoint);

        // Parse simple value using proper word parsing that handles quotes
        let value_str = array_element_value(ctx).parse_next(input)?;

        Ok((None, ast::Word::new(&value_str)))
    }
}

/// Parse an assignment word (VAR=value or VAR+=value or VAR[idx]=value or VAR=(array elements))
/// Returns (Assignment, original word as `ast::Word`)
fn assignment_word<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (ast::Assignment, ast::Word), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse variable name (must start with letter or underscore)
        let var_name = (
            winnow::token::one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
            winnow::token::take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
        )
            .take()
            .parse_next(input)?;

        // Check for array element syntax: var[index]
        let array_index = if winnow::combinator::opt::<_, _, PError, _>('[')
            .parse_next(input)?
            .is_some()
        {
            // Parse the index (everything until ']')
            let index = winnow::token::take_while(1.., |c: char| c != ']').parse_next(input)?;
            ']'.parse_next(input)?;
            Some(index.to_string())
        } else {
            None
        };

        // Check for optional '+' (append assignment)
        let append = winnow::combinator::opt('+').parse_next(input)?.is_some();

        // Must have '='
        '='.parse_next(input)?;

        // Check if it's an array assignment
        let checkpoint = input.checkpoint();
        let has_paren = winnow::combinator::opt::<_, _, PError, _>('(')
            .parse_next(input)?
            .is_some();

        if has_paren {
            // Parse array elements
            let mut elements = Vec::new();
            let mut full_word = String::with_capacity(var_name.len() + 16);
            full_word.push_str(var_name);
            if append {
                full_word.push_str("+=(");
            } else {
                full_word.push_str("=(");
            }

            loop {
                // Inside array literals, newlines act as whitespace separators
                // (just like spaces/tabs). Consume all whitespace including newlines.
                array_spaces().parse_next(input)?;

                // Check for closing paren
                if winnow::combinator::opt::<_, _, PError, _>(')')
                    .parse_next(input)?
                    .is_some()
                {
                    full_word.push(')');
                    break;
                }

                // Parse element
                let elem = array_element(ctx).parse_next(input)?;

                // Add to full_word
                if !elements.is_empty() {
                    full_word.push(' ');
                }
                if let Some(ref index) = elem.0 {
                    full_word.push('[');
                    full_word.push_str(&index.value);
                    full_word.push_str("]=");
                }
                full_word.push_str(&elem.1.value);

                elements.push(elem);
            }

            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);

            let assignment = ast::Assignment {
                name: ast::AssignmentName::VariableName(var_name.to_string()),
                value: ast::AssignmentValue::Array(elements),
                append,
                loc,
            };

            return Ok((assignment, ast::Word::new(&full_word)));
        }

        // Not an array, reset and parse scalar value
        input.reset(&checkpoint);

        // Parse the value using proper word parsing that handles quotes, escapes, etc.
        // The value can be empty (e.g., x=), so use opt
        let value_word = winnow::combinator::opt(word_as_ast(ctx, tracker)).parse_next(input)?;
        let value_str = value_word.as_ref().map_or("", |w| w.value.as_str());

        // Construct the full assignment word for AST
        let mut full_word = String::with_capacity(var_name.len() + value_str.len() + 10);
        full_word.push_str(var_name);
        if let Some(ref idx) = array_index {
            full_word.push('[');
            full_word.push_str(idx);
            full_word.push(']');
        }
        if append {
            full_word.push_str("+=");
        } else {
            full_word.push('=');
        }
        full_word.push_str(value_str);

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        // Use ArrayElementName if we have an index, otherwise VariableName
        let name = if let Some(idx) = array_index {
            ast::AssignmentName::ArrayElementName(var_name.to_string(), idx)
        } else {
            ast::AssignmentName::VariableName(var_name.to_string())
        };

        let assignment = ast::Assignment {
            name,
            value: ast::AssignmentValue::Scalar(ast::Word::new(value_str)),
            append,
            loc,
        };

        let word = ast::Word::new(&full_word);

        Ok((assignment, word))
    }
}

/// Parse `cmd_prefix` (assignments and redirects before command name)
/// Corresponds to: peg.rs `cmd_prefix()`
pub fn cmd_prefix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandPrefix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::terminated(
                winnow::combinator::alt((
                    io_redirect(ctx, tracker)
                        .map(|r| ast::CommandPrefixOrSuffixItem::IoRedirect(r.redirect)),
                    assignment_word(ctx, tracker).map(|(assignment, word)| {
                        ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
                    }),
                )),
                spaces(),
            ),
        )
        .map(ast::CommandPrefix)
        .parse_next(input)
    }
}

/// Check if we're at a here-doc marker (<<) but NOT a here-string (<<<)
fn at_here_doc_marker<'a>() -> impl Parser<StrStream<'a>, (), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Skip optional fd number
        winnow::combinator::opt(io_number()).parse_next(input)?;

        // Check for << but not <<<
        let checkpoint = input.checkpoint();
        if winnow::combinator::opt::<_, _, PError, _>("<<")
            .parse_next(input)?
            .is_some()
        {
            // Make sure it's not <<<
            if winnow::combinator::peek::<_, _, PError, _>('<')
                .parse_next(input)
                .is_ok()
            {
                // It's <<<, not <<
                input.reset(&checkpoint);
                return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
            }
            input.reset(&checkpoint);
            Ok(())
        } else {
            input.reset(&checkpoint);
            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
        }
    }
}

/// Parse multiple here-docs when we know we're at a here-doc marker.
/// Returns a Vec of `IoRedirect` items.
fn parse_here_docs<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::CommandPrefixOrSuffixItem>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (docs, remaining) = here_documents(tracker).parse_next(input)?;

        // Store trailing content in context for later processing by pipe_sequence
        if let Some(trailing) = remaining {
            *ctx.pending_heredoc_trailing.borrow_mut() = Some(trailing);
        }

        let items: Vec<ast::CommandPrefixOrSuffixItem> = docs
            .into_iter()
            .map(|(fd, doc)| {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::HereDocument(fd, doc))
            })
            .collect();

        Ok(items)
    }
}

/// Parse a single suffix item (word, redirect, process substitution, or assignment).
fn single_suffix_item<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandPrefixOrSuffixItem, PError> + 'a {
    winnow::combinator::alt((
        io_redirect(ctx, tracker).map(|r| ast::CommandPrefixOrSuffixItem::IoRedirect(r.redirect)),
        process_substitution(ctx, tracker)
            .map(|(kind, cmd)| ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, cmd)),
        assignment_word(ctx, tracker).map(|(assignment, word)| {
            ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
        }),
        word_as_ast(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::Word),
    ))
}

/// Parse `cmd_suffix` (arguments and redirections).
///
/// Now supports words, redirections, and process substitutions.
/// Handles multiple here-docs on the same line properly.
/// Corresponds to: winnow.rs `cmd_suffix()`.
pub fn cmd_suffix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandSuffix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Require at least one space before suffix items
        spaces1().parse_next(input)?;

        let mut all_items: Vec<ast::CommandPrefixOrSuffixItem> = Vec::new();

        loop {
            // Fast path: peek at first char to decide what to try
            let Ok(ch) = peek_char().parse_next(input) else {
                break;
            };

            // Only check for here-docs when we see '<' or a digit (fd number)
            if ch == '<' || ch.is_ascii_digit() {
                // Check if this is a here-doc (but not here-string)
                if winnow::combinator::peek::<_, _, PError, _>(at_here_doc_marker())
                    .parse_next(input)
                    .is_ok()
                {
                    let items = parse_here_docs(ctx, tracker).parse_next(input)?;
                    all_items.extend(items);
                    // Heredoc resolution consumed the command-line newline and
                    // all heredoc content lines.  Any trailing content on the
                    // same line (e.g., "| grep") is in pending_heredoc_trailing
                    // and handled by pipe_sequence.  We must stop parsing
                    // suffix items here — the next line is a new command.
                    break;
                }
            }

            // Try to parse a single suffix item
            match single_suffix_item(ctx, tracker).parse_next(input) {
                Ok(item) => {
                    all_items.push(item);
                    spaces().parse_next(input)?;
                }
                Err(_) => break,
            }
        }

        if all_items.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(ast::CommandSuffix(all_items))
    }
}

/// Parse a simple command (command name + optional arguments)
/// Now supports: prefix (assignments/redirects) + optional command + optional suffix
/// Corresponds to: peg.rs `simple_command()`
pub fn simple_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::SimpleCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Try to parse optional prefix (assignments and/or redirects)
        let prefix = winnow::combinator::opt(cmd_prefix(ctx, tracker)).parse_next(input)?;

        // Try to parse optional command name (must not be reserved word).
        // N.B. Must use opt() rather than .ok() so the input position is
        // restored on failure — .ok() discards errors without backtracking.
        let word_or_name =
            winnow::combinator::opt(non_reserved_word(ctx, tracker)).parse_next(input)?;

        // Try to parse optional suffix (args and/or redirects)
        let suffix = winnow::combinator::opt(cmd_suffix(ctx, tracker)).parse_next(input)?;

        // Must have at least one of: prefix, word, or suffix
        if prefix.is_none() && word_or_name.is_none() && suffix.is_none() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(ast::SimpleCommand {
            prefix,
            word_or_name,
            suffix,
        })
    }
}

/// Helper: Parse optional redirects after a compound command
/// Optimized to peek for redirect operators before attempting parse
fn optional_redirects<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Option<ast::RedirectList>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Peek ahead to check for redirect operators (after consuming whitespace)
        // Scan past any spaces/tabs to find the next meaningful character
        let remaining = input.as_ref().trim_start_matches([' ', '\t'].as_ref());

        // Check if next char is a redirect operator or digit (for fd redirects like 2>)
        let has_redirect = remaining
            .chars()
            .next()
            .is_some_and(|c| c == '<' || c == '>' || c.is_ascii_digit());

        if has_redirect {
            winnow::combinator::opt(redirect_list(ctx, tracker)).parse_next(input)
        } else {
            Ok(None)
        }
    }
}

/// Peek the first word without consuming input (for keyword dispatch)
fn peek_first_word<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    winnow::combinator::peek(take_while(1.., |c: char| c.is_alphanumeric() || c == '_'))
}

/// Parse a command (simple or compound).
///
/// Corresponds to: winnow.rs `command()`.
/// Uses keyword dispatch for performance - dispatches based on first word/char
/// to avoid trying all compound command parsers for simple commands.
#[allow(clippy::too_many_lines)]
pub fn command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Command, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?; // Consume optional leading spaces

        // Fast path: dispatch based on first character
        let Ok(first_char) = peek_char().parse_next(input) else {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        };

        match first_char {
            // Brace group: { ... }
            '{' => (brace_group(ctx, tracker), optional_redirects(ctx, tracker))
                .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::BraceGroup(c), r))
                .parse_next(input),

            // Parenthesized: subshell ( ... ) or arithmetic (( ... ))
            '(' => (
                paren_compound(ctx, tracker),
                optional_redirects(ctx, tracker),
            )
                .map(|(c, r)| ast::Command::Compound(c, r))
                .parse_next(input),

            // Extended test: [[ ... ]] (bash mode only)
            '[' if !ctx.options.posix_mode && !ctx.options.sh_mode => {
                // Check if it's [[
                if winnow::combinator::peek::<_, _, PError, _>("[[")
                    .parse_next(input)
                    .is_ok()
                {
                    (
                        extended_test_command(ctx, tracker),
                        optional_redirects(ctx, tracker),
                    )
                        .map(|(cmd, r)| ast::Command::ExtendedTest(cmd, r))
                        .parse_next(input)
                } else {
                    // Single [ is the test command (simple command)
                    simple_command(ctx, tracker)
                        .map(ast::Command::Simple)
                        .parse_next(input)
                }
            }

            // Alphabetic: could be keyword or simple command
            c if c.is_alphabetic() || c == '_' => {
                // Peek the first word to dispatch on keywords
                if let Ok(word) = peek_first_word().parse_next(input) {
                    match word {
                        "if" => (if_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::IfClause(c), r)
                            })
                            .parse_next(input),
                        "while" => (while_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::WhileClause(c), r)
                            })
                            .parse_next(input),
                        "until" => (until_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::UntilClause(c), r)
                            })
                            .parse_next(input),
                        "for" => (
                            for_or_arithmetic_for(ctx, tracker),
                            optional_redirects(ctx, tracker),
                        )
                            .map(|(c, r)| ast::Command::Compound(c, r))
                            .parse_next(input),
                        "case" => (case_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::CaseClause(c), r)
                            })
                            .parse_next(input),
                        "function" => function_definition(ctx, tracker)
                            .map(ast::Command::Function)
                            .parse_next(input),
                        // Reserved words that terminate compound commands - fail cleanly
                        "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "in" => {
                            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
                        }
                        // Not a keyword - check if it looks like a function definition (name followed by ())
                        _ => {
                            // Peek for function definition pattern: name + optional_spaces + "()"
                            // Function names may contain hyphens, dots, and other
                            // non-metacharacters (see bash manual, §Shell Functions).
                            let is_func_def = winnow::combinator::peek::<_, _, PError, _>((
                                take_while(1.., |c: char| {
                                    c.is_alphanumeric()
                                        || matches!(c, '_' | '-' | '.' | ':' | '+' | '@')
                                }),
                                take_while(0.., |c| c == ' ' || c == '\t'),
                                "()",
                            ))
                            .parse_next(input)
                            .is_ok();

                            if is_func_def {
                                winnow::combinator::alt((
                                    function_definition(ctx, tracker).map(ast::Command::Function),
                                    simple_command(ctx, tracker).map(ast::Command::Simple),
                                ))
                                .parse_next(input)
                            } else {
                                // Regular command - try simple command first
                                winnow::combinator::alt((
                                    simple_command(ctx, tracker).map(ast::Command::Simple),
                                    function_definition(ctx, tracker).map(ast::Command::Function),
                                ))
                                .parse_next(input)
                            }
                        }
                    }
                } else {
                    // Can't peek word, try simple command
                    simple_command(ctx, tracker)
                        .map(ast::Command::Simple)
                        .parse_next(input)
                }
            }

            // Other characters: likely simple command (variable assignment, redirect, etc.)
            _ => winnow::combinator::alt((
                simple_command(ctx, tracker).map(ast::Command::Simple),
                function_definition(ctx, tracker).map(ast::Command::Function),
            ))
            .parse_next(input),
        }
    }
}

// ============================================================================
// Tier 4: Pipelines
// ============================================================================

/// Parse pipe operator ('|' or '|&')
/// Corresponds to: winnow.rs `pipe_operator()`
/// Returns true if it's |& (pipe stderr too)
#[inline]
pub fn pipe_operator<'a>() -> impl Parser<StrStream<'a>, bool, PError> {
    // Note: Keep alt() for 2 alternatives - dispatch! is slower due to peek overhead
    winnow::combinator::alt((
        "|&".value(true), // |& pipes both stdout and stderr
        "|".value(false), // | pipes only stdout
    ))
}

/// Add stderr redirect (2>&1) to a command for |& support
fn add_pipe_extension_redirect(cmd: &mut ast::Command) {
    let redirect = ast::IoRedirect::File(
        Some(2), // stderr
        ast::IoFileRedirectKind::DuplicateOutput,
        ast::IoFileRedirectTarget::Fd(1), // redirect to stdout
    );

    match cmd {
        ast::Command::Simple(simple) => {
            let redirect_item = ast::CommandPrefixOrSuffixItem::IoRedirect(redirect);
            if let Some(suffix) = &mut simple.suffix {
                suffix.0.push(redirect_item);
            } else {
                simple.suffix = Some(ast::CommandSuffix(vec![redirect_item]));
            }
        }
        ast::Command::Compound(_, redirect_list) => {
            if let Some(list) = redirect_list {
                list.0.push(redirect);
            } else {
                *redirect_list = Some(ast::RedirectList(vec![redirect]));
            }
        }
        ast::Command::Function(func) => {
            if let Some(list) = &mut func.body.1 {
                list.0.push(redirect);
            } else {
                func.body.1 = Some(ast::RedirectList(vec![redirect]));
            }
        }
        ast::Command::ExtendedTest(_, rlist) => {
            // Add redirect to extended test
            if let Some(rlist) = rlist {
                rlist.0.push(redirect);
            } else {
                *rlist = Some(ast::RedirectList(vec![redirect]));
            }
        }
    }
}

/// Parse a single command from a string (used for trailing here-doc content)
fn parse_trailing_command(input: &str, options: &ParserOptions) -> Option<ast::Command> {
    let source_info = SourceInfo::default();
    let pending = std::cell::RefCell::new(None);
    let ctx = ParseContext {
        options,
        source_info: &source_info,
        pending_heredoc_trailing: &pending,
    };
    let tracker = PositionTracker::new(input);
    let mut stream = LocatingSlice::new(input);
    command(&ctx, &tracker).parse_next(&mut stream).ok()
}

/// Parse pipe sequence (command | command | command)
/// Corresponds to: winnow.rs `pipe_sequence()`
pub fn pipe_sequence<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::Command>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (first, rest) = (
            command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(
                0..,
                (
                    winnow::combinator::preceded(spaces(), pipe_operator()), // spaces then |
                    winnow::combinator::preceded((linebreak(), spaces()), command(ctx, tracker)), // optional newlines+spaces then command
                ),
            ),
        )
            .parse_next(input)?;

        // Build initial commands vector
        let mut commands =
            rest.into_iter()
                .fold(vec![first], |mut commands, (is_pipe_and, cmd)| {
                    if is_pipe_and {
                        // For |&, add 2>&1 redirect to the previous command
                        if let Some(prev_cmd) = commands.last_mut() {
                            add_pipe_extension_redirect(prev_cmd);
                        }
                    }
                    commands.push(cmd);
                    commands
                });

        // Check if there's pending trailing content from a here-doc (e.g., "| grep hello")
        if let Some(trailing) = ctx.pending_heredoc_trailing.borrow_mut().take() {
            // Parse the trailing content as additional pipeline commands
            if let Some(stripped) = trailing.strip_prefix('|') {
                let trailing_input = format!("{}\n", stripped.trim());
                if let Some(trailing_cmd) = parse_trailing_command(&trailing_input, ctx.options) {
                    commands.push(trailing_cmd);
                }
            }
        }

        Ok(commands)
    }
}

/// Parse optional time keyword with optional -p flag
/// Returns Option<PipelineTimed>
fn pipeline_timed<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Option<ast::PipelineTimed>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Try to parse "time" keyword
        if keyword("time").parse_next(input).is_err() {
            return Ok(None);
        }

        // Consume spaces after "time"
        spaces().parse_next(input)?;

        // Check for optional "-p" flag
        let has_posix_flag =
            winnow::combinator::opt(winnow::combinator::terminated("-p", spaces()))
                .parse_next(input)?
                .is_some();

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        let timed = if has_posix_flag {
            ast::PipelineTimed::TimedWithPosixOutput(loc)
        } else {
            ast::PipelineTimed::Timed(loc)
        };

        Ok(Some(timed))
    }
}

/// Parse optional bang (!) operators before a pipeline
/// Returns the count of bang operators
fn pipeline_bang<'a>() -> impl Parser<StrStream<'a>, usize, PError> {
    winnow::combinator::repeat(0.., winnow::combinator::terminated(keyword("!"), spaces()))
        .map(|bangs: Vec<_>| bangs.len())
}

/// Parse a pipeline
/// Corresponds to: winnow.rs `pipeline()` with full support for time and bang
pub fn pipeline<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Pipeline, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            pipeline_timed(tracker),
            pipeline_bang(),
            pipe_sequence(ctx, tracker),
        )
            .map(|(timed, bang_count, seq)| {
                ast::Pipeline {
                    timed,
                    bang: bang_count % 2 == 1, // Odd number of bangs = inverted
                    seq,
                }
            })
            .parse_next(input)
    }
}

// ============================================================================
// Tier 5: And/Or Lists
// ============================================================================

/// Parse and/or operator ('&&' or '||')
/// Corresponds to: winnow.rs `and_or_op()`
/// Returns true for And (&&), false for Or (||)
#[inline]
pub fn and_or_op<'a>() -> impl Parser<StrStream<'a>, bool, PError> {
    // Note: Keep alt() for 2 alternatives - dispatch! is slower due to peek overhead
    winnow::combinator::alt((
        "&&".value(true),  // And operator
        "||".value(false), // Or operator
    ))
}

/// Parse and/or continuation (operator + pipeline)
/// Corresponds to: winnow.rs `and_or_continuation()`
fn and_or_continuation<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::AndOr, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            winnow::combinator::preceded((linebreak(), spaces()), and_or_op()), // optional newlines+spaces, then operator
            winnow::combinator::preceded((linebreak(), spaces()), pipeline(ctx, tracker)), // optional newlines+spaces, then pipeline
        )
            .map(|(is_and, pipe): (bool, ast::Pipeline)| {
                if is_and {
                    ast::AndOr::And(pipe)
                } else {
                    ast::AndOr::Or(pipe)
                }
            })
            .parse_next(input)
    }
}

/// Parse and/or list (pipelines connected with && or ||)
/// Corresponds to: winnow.rs `and_or()`
pub fn and_or<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::AndOrList, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            pipeline(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(0.., and_or_continuation(ctx, tracker)),
        )
            .map(
                |(first, additional): (ast::Pipeline, Vec<ast::AndOr>)| ast::AndOrList {
                    first,
                    additional,
                },
            )
            .parse_next(input)
    }
}

// ============================================================================
// Tier 10: Subshells and Command Groups
// ============================================================================

/// Parse a compound list (used inside subshells, brace groups, etc.)
///
/// Similar to `complete_command` but with optional leading linebreaks and more flexible separators
/// Corresponds to: winnow.rs `compound_list()`
pub fn compound_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompoundList, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Optional leading linebreaks
        linebreak().parse_next(input)?;

        // Parse first and_or (required)
        let mut current_ao = and_or(ctx, tracker).parse_next(input)?;
        let mut items: Vec<ast::CompoundListItem> = vec![];

        // Try to parse (separator + and_or) pairs
        // Note: Manual loop is faster than repeat() combinator here due to early break optimization
        loop {
            // Try to get separator after current and_or (handles both ; & and newlines)
            spaces().parse_next(input)?;

            let sep_opt = if let Ok(sep_opt) = separator().parse_next(input) {
                spaces().parse_next(input)?;
                sep_opt
            } else {
                // No separator - add current and_or with default separator and we're done
                items.push(ast::CompoundListItem(
                    current_ao,
                    ast::SeparatorOperator::Sequence,
                ));
                break;
            };

            // Convert Option<SeparatorOperator> to SeparatorOperator (None means newline, treat as
            // Sequence)
            let sep = sep_opt.unwrap_or(ast::SeparatorOperator::Sequence);

            // Push current and_or with its separator
            items.push(ast::CompoundListItem(current_ao, sep));

            // We have a separator, check if there's another and_or after it
            if let Ok(next_ao) = and_or(ctx, tracker).parse_next(input) {
                // Move to next
                current_ao = next_ao;
            } else {
                // Trailing separator
                break;
            }
        }

        Ok(ast::CompoundList(items))
    }
}

/// Parse a subshell: ( commands )
/// Corresponds to: winnow.rs `subshell()`
pub fn subshell<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::SubshellCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            ('(', spaces(), linebreak()),
            compound_list(ctx, tracker),
            (linebreak(), spaces(), ')'),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::SubshellCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse a brace group: { commands; }
/// Corresponds to: winnow.rs `brace_group()`
pub fn brace_group<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::BraceGroupCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            // IMPORTANT: Require at least one space OR newline after '{'
            // This distinguishes brace groups from brace expansion:
            // - Brace group: { echo hello; } (requires space after {)
            // - Brace expansion: {1..10} (no space, part of word)
            (
                '{',
                winnow::combinator::alt((
                    spaces1(),        // At least one space/tab
                    newline().void(), // Or a newline
                )),
            ),
            compound_list(ctx, tracker),
            // Before '}': optional linebreak and spaces
            // Note: A separator (;/&) or newline is required before }, but that's
            // handled by compound_list. We just allow optional additional whitespace.
            (linebreak(), spaces(), '}'),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::BraceGroupCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse process substitution: <(command) or >(command)
/// Corresponds to: peg.rs `process_substitution()`
pub fn process_substitution<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (ast::ProcessSubstitutionKind, ast::SubshellCommand), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse < or > to determine the kind
        let kind = winnow::combinator::alt((
            "<".value(ast::ProcessSubstitutionKind::Read),
            ">".value(ast::ProcessSubstitutionKind::Write),
        ))
        .parse_next(input)?;

        // Then parse the subshell-like content: ( compound_list )
        let list = winnow::combinator::delimited(
            ('(', spaces(), linebreak()),
            compound_list(ctx, tracker),
            (linebreak(), spaces(), ')'),
        )
        .parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok((kind, ast::SubshellCommand { list, loc }))
    }
}

// ============================================================================
// Helper parsers for compound commands
// ============================================================================

/// Parse a sequential separator (semicolon or newlines)
/// Corresponds to: winnow.rs `sequential_sep()`
#[inline]
pub fn sequential_sep<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    winnow::combinator::alt(((';', linebreak()).void(), newline_list().void()))
}

/// Check if a string is a valid shell variable name
/// Names must start with [a-zA-Z_] and contain only [a-zA-Z0-9_]
fn is_valid_name(s: &str) -> bool {
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

/// Parse a valid variable name
/// Corresponds to: winnow.rs `name()`
pub fn name<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::preceded(spaces(), bare_word())
        .verify(|s: &str| is_valid_name(s))
        .map(|s: &str| s.to_string())
}

/// Check if a string is a valid bash function name.
///
/// Bash function names may contain any characters that are valid in a word
/// (including hyphens and dots), unlike variable names which are restricted
/// to `[a-zA-Z_][a-zA-Z0-9_]*`.  The only restrictions are that the name
/// must not be empty, must start with a letter or underscore, and must not
/// end with `=` (to avoid ambiguity with assignments).
fn is_valid_fname(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.ends_with('=') {
        return false;
    }
    let first = s.chars().next().unwrap();
    first.is_ascii_alphabetic() || first == '_'
}

/// Parse a function name.
fn fname<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::preceded(spaces(), bare_word())
        .verify(|s: &str| is_valid_fname(s))
        .map(|s: &str| s.to_string())
}

// ============================================================================
// Tier 11: Compound Commands (if, while, until, for, case)
// ============================================================================

/// Match a specific keyword (shell reserved word)
/// Keywords must be followed by a delimiter (space, tab, newline, semicolon, etc.)
/// to avoid matching them as part of a larger word
fn keyword<'a>(word: &'static str) -> impl Parser<StrStream<'a>, &'a str, PError> {
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

/// Parse a do group: do ... done
/// Corresponds to: winnow.rs `do_group()`
pub fn do_group<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::DoGroupCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            keyword("do"),
            compound_list(ctx, tracker), // compound_list handles its own leading linebreak
            keyword("done"),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::DoGroupCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse an elif clause
fn elif_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ElseClause, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        keyword("elif").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        keyword("then").parse_next(input)?;
        let body = compound_list(ctx, tracker).parse_next(input)?;
        Ok(ast::ElseClause {
            condition: Some(condition),
            body,
        })
    }
}

/// Parse an else clause (final, no condition)
fn else_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ElseClause, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        keyword("else").parse_next(input)?;
        let body = compound_list(ctx, tracker).parse_next(input)?;
        Ok(ast::ElseClause {
            condition: None,
            body,
        })
    }
}

/// Parse an if clause: if ... then ... [elif ... then ...]* [else ...] fi
/// Corresponds to: winnow.rs `if_clause()`
pub fn if_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::IfClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("if").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        keyword("then").parse_next(input)?;
        let then_body = compound_list(ctx, tracker).parse_next(input)?;

        // Parse elif clauses (zero or more)
        let mut elses: Vec<ast::ElseClause> =
            repeat(0.., elif_clause(ctx, tracker)).parse_next(input)?;

        // Parse optional else clause
        if let Ok(else_part) = else_clause(ctx, tracker).parse_next(input) {
            elses.push(else_part);
        }

        keyword("fi").parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::IfClauseCommand {
            condition,
            then: then_body,
            elses: if elses.is_empty() { None } else { Some(elses) },
            loc,
        })
    }
}

/// Parse a while clause: while ... do ... done
/// Corresponds to: winnow.rs `while_clause()`
pub fn while_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::WhileOrUntilClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("while").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        let body = do_group(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

/// Parse an until clause: until ... do ... done
/// Corresponds to: winnow.rs `until_clause()`
pub fn until_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::WhileOrUntilClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("until").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        let body = do_group(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

// ============================================================================
// Tier 12: For Loops
// ============================================================================

/// Parse a for clause: for var in list; do ... done
/// Corresponds to: winnow.rs `for_clause()`
pub fn for_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ForClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("for").parse_next(input)?;
        let var_name = name().parse_next(input)?;

        linebreak().parse_next(input)?;

        // Optional "in" wordlist
        let values = if keyword("in").parse_next(input).is_ok() {
            // Parse space-separated words (preceded by spaces to consume leading space after "in")
            winnow::combinator::opt(winnow::combinator::preceded(
                spaces(),
                winnow::combinator::separated(1.., word_as_ast(ctx, tracker), spaces1()),
            ))
            .parse_next(input)?
        } else {
            None
        };

        sequential_sep().parse_next(input)?;
        let body = do_group(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ForClauseCommand {
            variable_name: var_name,
            values,
            body,
            loc,
        })
    }
}

// ============================================================================
// Tier 13: Case Statements
// ============================================================================

/// Parse case item terminator (;;, ;&, or ;;&)
fn case_item_terminator<'a>() -> impl Parser<StrStream<'a>, ast::CaseItemPostAction, PError> {
    winnow::combinator::preceded(
        spaces(),
        dispatch! {peek_op3();
            ";;&" => ";;&".value(ast::CaseItemPostAction::ContinueEvaluatingCases),
            ";&" => ";&".value(ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem),
            ";;" => ";;".value(ast::CaseItemPostAction::ExitCase),
            _ => fail,
        },
    )
}

/// Parse a case item: pattern) commands ;;
fn case_item<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CaseItem, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?;
        let start_offset = tracker.offset_from_locating(input);

        // Optional leading (
        let _ = winnow::combinator::opt::<_, _, PError, _>('(').parse_next(input)?;
        spaces().parse_next(input)?;

        // Parse patterns: word separated by |
        let patterns: Vec<ast::Word> = winnow::combinator::separated(
            1..,
            word_as_ast(ctx, tracker),
            winnow::combinator::preceded(spaces(), winnow::combinator::terminated('|', spaces())),
        )
        .parse_next(input)?;

        spaces().parse_next(input)?;
        ')'.parse_next(input)?;

        linebreak().parse_next(input)?;

        // Parse body (optional)
        let cmd = winnow::combinator::opt(compound_list(ctx, tracker)).parse_next(input)?;

        // Parse case item terminator (optional - default to ExitCase)
        let post_action = winnow::combinator::opt(case_item_terminator())
            .parse_next(input)?
            .unwrap_or(ast::CaseItemPostAction::ExitCase);

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        linebreak().parse_next(input)?;

        Ok(ast::CaseItem {
            patterns,
            cmd,
            post_action,
            loc: Some(loc),
        })
    }
}

/// Parse case list (multiple case items until "esac")
fn case_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::CaseItem>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let mut items = vec![];

        loop {
            // Peek ahead to see if we have "esac"
            let checkpoint = input.checkpoint();
            spaces().parse_next(input)?;
            if keyword("esac").parse_next(input).is_ok() {
                // Found esac, restore and break
                input.reset(&checkpoint);
                break;
            }
            input.reset(&checkpoint);

            // Parse case item
            match case_item(ctx, tracker).parse_next(input) {
                Ok(item) => items.push(item),
                Err(_) => break,
            }
        }

        if items.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(items)
    }
}

/// Parse a case clause: case word in patterns) commands ;; esac
/// Corresponds to: winnow.rs `case_clause()`
pub fn case_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CaseClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("case").parse_next(input)?;
        spaces().parse_next(input)?;
        let target = word_as_ast(ctx, tracker).parse_next(input)?;

        linebreak().parse_next(input)?;
        keyword("in").parse_next(input)?;
        linebreak().parse_next(input)?;

        // Use opt() for optional case list
        let items = winnow::combinator::opt(case_list(ctx, tracker)).parse_next(input)?;

        spaces().parse_next(input)?;
        keyword("esac").parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::CaseClauseCommand {
            value: target,
            cases: items.unwrap_or_default(),
            loc,
        })
    }
}

// ============================================================================
// Tier 15: Arithmetic Expressions
// ============================================================================

/// Parse arithmetic expression inside (( ))
/// Collects all content until ")) or ; (at depth 0) is found, tracking paren depth
/// Corresponds to: winnow.rs `arithmetic_expression()`
/// Normalize an arithmetic expression string to match peg parser output.
/// The peg parser uses tokenizer which treats some characters as operators (like <, >, |, &)
/// and others as word characters (like =, +, -, *, /).
/// Spaces are only preserved between adjacent word tokens.
fn normalize_arithmetic_expr(s: &str) -> String {
    // Shell operators in arithmetic context (matches tokenizer's is_operator list)
    const SHELL_OPERATORS: &[char] = &['<', '>', '|', '&', '(', ')', ';'];

    let s = s.trim();
    let mut result = String::with_capacity(s.len());
    let mut last_was_word = false;
    let mut pending_space = false;

    for c in s.chars() {
        if c.is_whitespace() {
            // Mark that we saw a space, but don't emit yet
            pending_space = true;
            continue;
        }

        // Shell operators suppress spaces around them
        let is_shell_op = SHELL_OPERATORS.contains(&c);

        if is_shell_op {
            // Operators don't get spaces around them
            pending_space = false;
            last_was_word = false;
        } else {
            // Non-operator: emit pending space if last was also non-operator
            if pending_space && last_was_word {
                result.push(' ');
            }
            pending_space = false;
            last_was_word = true;
        }

        result.push(c);
    }

    result
}

fn arithmetic_expression<'a>() -> impl Parser<StrStream<'a>, ast::UnexpandedArithmeticExpr, PError>
{
    move |input: &mut StrStream<'a>| {
        let mut expr_str = String::new();
        let mut paren_depth = 0;

        loop {
            // Check for end at depth 0
            if paren_depth == 0 {
                let checkpoint = input.checkpoint();
                // Skip optional spaces to peek ahead
                spaces().parse_next(input)?;

                // Check for "))" - allow optional space between to match peg tokenizer behavior
                if winnow::combinator::opt::<_, _, PError, _>((')', spaces(), ')'))
                    .parse_next(input)?
                    .is_some()
                {
                    input.reset(&checkpoint);
                    break;
                }

                // Check for ";" (for arithmetic for loops)
                if winnow::combinator::opt::<_, _, PError, _>(';')
                    .parse_next(input)?
                    .is_some()
                {
                    input.reset(&checkpoint);
                    break;
                }

                input.reset(&checkpoint);
            }

            // Get next character
            let checkpoint = input.checkpoint();

            // Try to match '('
            if winnow::combinator::opt::<_, _, PError, _>('(')
                .parse_next(input)?
                .is_some()
            {
                paren_depth += 1;
                expr_str.push('(');
                continue;
            }
            input.reset(&checkpoint);

            // Try to match ')'
            if winnow::combinator::opt::<_, _, PError, _>(')')
                .parse_next(input)?
                .is_some()
            {
                paren_depth -= 1;
                expr_str.push(')');
                continue;
            }
            input.reset(&checkpoint);

            // Match any other character that's not )) or ;
            let c_opt: Result<char, PError> = winnow::token::any.parse_next(input);
            if let Ok(c) = c_opt {
                expr_str.push(c);
            } else {
                break;
            }
        }

        Ok(ast::UnexpandedArithmeticExpr {
            value: normalize_arithmetic_expr(&expr_str),
        })
    }
}

/// Parse arithmetic command (( expr ))
/// Corresponds to: winnow.rs `arithmetic_command()`
pub fn arithmetic_command<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ArithmeticCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse (( - allow optional whitespace between them to match peg tokenizer behavior
        // (the tokenizer produces separate ( tokens even with spaces)
        '('.parse_next(input)?;
        spaces().parse_next(input)?;
        '('.parse_next(input)?;

        // Parse expression
        let expr = arithmetic_expression().parse_next(input)?;

        // Parse )) - allow optional whitespace between them
        spaces().parse_next(input)?;
        ')'.parse_next(input)?;
        spaces().parse_next(input)?;
        ')'.parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ArithmeticCommand { expr, loc })
    }
}

/// Parse commands starting with '(' - either arithmetic (( )) or subshell ( )
/// Corresponds to: winnow.rs `paren_compound()`
fn paren_compound<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompoundCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // In POSIX or SH mode, only allow subshells (no arithmetic commands)
        if ctx.options.posix_mode || ctx.options.sh_mode {
            subshell(ctx, tracker)
                .map(ast::CompoundCommand::Subshell)
                .parse_next(input)
        } else {
            // In Bash mode, try arithmetic command (( first, then fall back to subshell
            winnow::combinator::alt((
                // Try (( first for arithmetic
                arithmetic_command(tracker).map(ast::CompoundCommand::Arithmetic),
                // Fall back to subshell
                subshell(ctx, tracker).map(ast::CompoundCommand::Subshell),
            ))
            .parse_next(input)
        }
    }
}

// ============================================================================
// Tier 16: Arithmetic For Loops
// ============================================================================

/// Parse arithmetic for body (`do_group` or `brace_group`)
/// Corresponds to: winnow.rs `arithmetic_for_body()` and peg.rs `arithmetic_for_body()`
/// Accepts: "; do", "\n do", or just " do" (spaces are consumed by keyword("do"))
fn arithmetic_for_body<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::DoGroupCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::alt((
            // Try sequential_sep followed by do_group (for "; do" or "\n do")
            winnow::combinator::preceded(sequential_sep(), do_group(ctx, tracker)),
            // Try do_group directly (for " do" - spaces consumed by keyword)
            do_group(ctx, tracker),
            // Try brace_group (convert to DoGroupCommand)
            brace_group(ctx, tracker).map(|bg| ast::DoGroupCommand {
                list: bg.list,
                loc: bg.loc,
            }),
        ))
        .parse_next(input)
    }
}

/// Parse arithmetic for clause: for (( init; cond; update )) body
/// Corresponds to: winnow.rs `arithmetic_for_clause()`
pub fn arithmetic_for_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ArithmeticForClauseCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse "for (("
        keyword("for").parse_next(input)?;
        spaces().parse_next(input)?;
        '('.parse_next(input)?;
        '('.parse_next(input)?;

        // Parse three arithmetic expressions separated by ;
        let initializer = winnow::combinator::opt(arithmetic_expression()).parse_next(input)?;
        spaces().parse_next(input)?;
        ';'.parse_next(input)?;

        let condition = winnow::combinator::opt(arithmetic_expression()).parse_next(input)?;
        spaces().parse_next(input)?;
        ';'.parse_next(input)?;

        let updater = winnow::combinator::opt(arithmetic_expression()).parse_next(input)?;

        // Parse "))"
        spaces().parse_next(input)?;
        ')'.parse_next(input)?;
        ')'.parse_next(input)?;

        // Parse body (arithmetic_for_body handles the sequential_sep)
        let body = arithmetic_for_body(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ArithmeticForClauseCommand {
            initializer,
            condition,
            updater,
            body,
            loc,
        })
    }
}

/// Parse commands starting with 'for' - either regular for or arithmetic for
/// Corresponds to: winnow.rs `for_or_arithmetic_for()`
fn for_or_arithmetic_for<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompoundCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // In POSIX or SH mode, only allow regular for loops
        if ctx.options.posix_mode || ctx.options.sh_mode {
            for_clause(ctx, tracker)
                .map(ast::CompoundCommand::ForClause)
                .parse_next(input)
        } else {
            // In Bash mode, try arithmetic for first, then fall back to regular for
            winnow::combinator::alt((
                // Try arithmetic for first: for ((
                arithmetic_for_clause(ctx, tracker).map(ast::CompoundCommand::ArithmeticForClause),
                // Fall back to regular for
                for_clause(ctx, tracker).map(ast::CompoundCommand::ForClause),
            ))
            .parse_next(input)
        }
    }
}

// ============================================================================
// Tier 17: Extended Test Expressions [[ ]]
// ============================================================================

/// Parse whitespace inside extended test [[ ]] expressions.
/// Unlike `spaces()`, this also handles newlines and backslash-newline continuations,
/// because bash allows multi-line [[ ]] expressions.
#[inline]
fn ext_test_spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    repeat::<_, _, (), _, _>(
        0..,
        winnow::combinator::alt((
            take_while(1.., |c: char| c == ' ' || c == '\t' || c == '\n').void(),
            ("\\", '\n').void(), // backslash-newline continuation
            comment(),           // # comments
        )),
    )
    .void()
}

/// Parse a unary test operator (-f, -z, -n, etc.)
/// Corresponds to: winnow.rs `parse_unary_operator()`
fn parse_unary_operator(op: &str) -> Option<ast::UnaryPredicate> {
    use ast::UnaryPredicate;
    match op {
        "-e" => Some(UnaryPredicate::FileExists),
        "-b" => Some(UnaryPredicate::FileExistsAndIsBlockSpecialFile),
        "-c" => Some(UnaryPredicate::FileExistsAndIsCharSpecialFile),
        "-d" => Some(UnaryPredicate::FileExistsAndIsDir),
        "-f" => Some(UnaryPredicate::FileExistsAndIsRegularFile),
        "-g" => Some(UnaryPredicate::FileExistsAndIsSetgid),
        "-h" | "-L" => Some(UnaryPredicate::FileExistsAndIsSymlink),
        "-k" => Some(UnaryPredicate::FileExistsAndHasStickyBit),
        "-p" => Some(UnaryPredicate::FileExistsAndIsFifo),
        "-r" => Some(UnaryPredicate::FileExistsAndIsReadable),
        "-s" => Some(UnaryPredicate::FileExistsAndIsNotZeroLength),
        "-t" => Some(UnaryPredicate::FdIsOpenTerminal),
        "-u" => Some(UnaryPredicate::FileExistsAndIsSetuid),
        "-w" => Some(UnaryPredicate::FileExistsAndIsWritable),
        "-x" => Some(UnaryPredicate::FileExistsAndIsExecutable),
        "-G" => Some(UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId),
        "-N" => Some(UnaryPredicate::FileExistsAndModifiedSinceLastRead),
        "-O" => Some(UnaryPredicate::FileExistsAndOwnedByEffectiveUserId),
        "-S" => Some(UnaryPredicate::FileExistsAndIsSocket),
        "-o" => Some(UnaryPredicate::ShellOptionEnabled),
        "-v" => Some(UnaryPredicate::ShellVariableIsSetAndAssigned),
        "-R" => Some(UnaryPredicate::ShellVariableIsSetAndNameRef),
        "-z" => Some(UnaryPredicate::StringHasZeroLength),
        "-n" => Some(UnaryPredicate::StringHasNonZeroLength),
        _ => None,
    }
}

/// Parse a binary test operator (=, !=, -eq, -lt, etc.)
/// Corresponds to: winnow.rs `parse_binary_operator()`
fn parse_binary_operator(op: &str) -> Option<ast::BinaryPredicate> {
    use ast::BinaryPredicate;
    match op {
        "=" | "==" => Some(BinaryPredicate::StringExactlyMatchesPattern),
        "!=" => Some(BinaryPredicate::StringDoesNotExactlyMatchPattern),
        "<" => Some(BinaryPredicate::LeftSortsBeforeRight),
        ">" => Some(BinaryPredicate::LeftSortsAfterRight),
        "-eq" => Some(BinaryPredicate::ArithmeticEqualTo),
        "-ne" => Some(BinaryPredicate::ArithmeticNotEqualTo),
        "-lt" => Some(BinaryPredicate::ArithmeticLessThan),
        "-le" => Some(BinaryPredicate::ArithmeticLessThanOrEqualTo),
        "-gt" => Some(BinaryPredicate::ArithmeticGreaterThan),
        "-ge" => Some(BinaryPredicate::ArithmeticGreaterThanOrEqualTo),
        "-nt" => Some(BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot),
        "-ot" => Some(BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes),
        "-ef" => Some(BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers),
        "=~" => Some(BinaryPredicate::StringMatchesRegex),
        _ => None,
    }
}

// ----------------------------------------------------------------------------
// Winnow-based Extended Test Expression Parsers
// ----------------------------------------------------------------------------

/// Parse a word in extended test context (bare word or quoted string with quotes preserved)
/// Consume characters into `out` until the matching `close` delimiter is found,
/// handling nested `open`/`close` pairs, quoted strings, and backslash escapes.
/// The closing delimiter is consumed and appended to `out`.
fn ext_test_consume_balanced(
    input: &mut StrStream<'_>,
    out: &mut String,
    open: char,
    close: char,
) -> Result<(), PError> {
    let mut depth: u32 = 1;
    while depth > 0 {
        let ch = winnow::token::any.parse_next(input)?;
        out.push(ch);
        match ch {
            c if c == open => depth += 1,
            c if c == close => depth -= 1,
            '\\' => {
                // Escape: consume next char too
                let escaped: Result<char, PError> = winnow::token::any.parse_next(input);
                if let Ok(c) = escaped {
                    out.push(c);
                }
            }
            '\'' => {
                // Single-quoted string: consume until closing quote
                loop {
                    let c = winnow::token::any.parse_next(input)?;
                    out.push(c);
                    if c == '\'' {
                        break;
                    }
                }
            }
            '"' => {
                // Double-quoted string: consume until closing quote, handling escapes
                loop {
                    let c = winnow::token::any.parse_next(input)?;
                    out.push(c);
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        let escaped: Result<char, PError> = winnow::token::any.parse_next(input);
                        if let Ok(ec) = escaped {
                            out.push(ec);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Parse a single-quoted string segment and append to word
fn ext_test_parse_single_quoted(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> Result<(), PError> {
    let quote: char = '\''.parse_next(input)?;
    let content: &str = take_while(0.., |c: char| c != '\'').parse_next(input)?;
    let end_quote: char = '\''.parse_next(input)?;
    word.push(quote);
    word.push_str(content);
    word.push(end_quote);
    Ok(())
}

/// Parse a double-quoted string segment and append to word
fn ext_test_parse_double_quoted(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> Result<(), PError> {
    let s = double_quoted_string().parse_next(input)?;
    word.push_str(&s);
    Ok(())
}

/// Parse a backslash escape sequence and append to word
fn ext_test_parse_backslash_escape(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> Result<(), PError> {
    winnow::token::any.parse_next(input)?;
    let escaped: Result<char, PError> = winnow::token::any.parse_next(input);
    if let Ok(c) = escaped {
        if c == '\n' {
            // Backslash-newline is line continuation — skip both
        } else {
            word.push('\\');
            word.push(c);
        }
    } else {
        word.push('\\');
    }
    Ok(())
}

/// Parse a dollar expansion ($var, $(...), $((...)), ${...}) and append to word
#[allow(clippy::branches_sharing_code)]
fn ext_test_parse_dollar_expansion(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> Result<(), PError> {
    word.push('$');
    winnow::token::any.parse_next(input)?;
    match peek_char().parse_next(input).ok() {
        Some('(') => {
            winnow::token::any.parse_next(input)?;
            // Check for $(( arithmetic )) vs $( command )
            if peek_char().parse_next(input).ok() == Some('(') {
                // $(( ... )) — arithmetic expansion
                word.push('(');
                word.push('(');
                winnow::token::any.parse_next(input)?;
                ext_test_consume_balanced(input, word, '(', ')')?;
                // Consume the second closing )
                if peek_char().parse_next(input).ok() == Some(')') {
                    word.push(')');
                    winnow::token::any.parse_next(input)?;
                }
            } else {
                // $( ... ) — command substitution
                word.push('(');
                ext_test_consume_balanced(input, word, '(', ')')?;
            }
        }
        Some('{') => {
            // ${ ... } — braced variable
            word.push('{');
            winnow::token::any.parse_next(input)?;
            ext_test_consume_balanced(input, word, '{', '}')?;
        }
        _ => {
            // $var or $!, $?, etc. — already consumed $
        }
    }
    Ok(())
}

/// Parse special characters (&, |, !) and handle accordingly
fn ext_test_parse_special_char(
    input: &mut StrStream<'_>,
    word: &mut String,
    ch: char,
) -> Result<bool, PError> {
    // Returns Ok(true) if parsing should continue, Ok(false) if should stop
    match ch {
        '&' => {
            let checkpoint = input.checkpoint();
            winnow::token::any.parse_next(input)?;
            if peek_char().parse_next(input).ok() == Some('&') {
                // This is &&, stop here
                input.reset(&checkpoint);
                Ok(false)
            } else {
                // Single &, include it
                input.reset(&checkpoint);
                word.push('&');
                winnow::token::any.parse_next(input)?;
                Ok(true)
            }
        }
        '|' => {
            let checkpoint = input.checkpoint();
            winnow::token::any.parse_next(input)?;
            if peek_char().parse_next(input).ok() == Some('|') {
                // This is ||, stop here
                input.reset(&checkpoint);
                Ok(false)
            } else {
                // Single |, include it
                input.reset(&checkpoint);
                word.push('|');
                winnow::token::any.parse_next(input)?;
                Ok(true)
            }
        }
        '!' => {
            let checkpoint = input.checkpoint();
            winnow::token::any.parse_next(input)?;
            if peek_char().parse_next(input).ok() == Some('=') {
                // This is !=, include both characters
                word.push('!');
                word.push('=');
                winnow::token::any.parse_next(input)?;
                Ok(true)
            } else {
                // Standalone !, stop here
                input.reset(&checkpoint);
                Ok(false)
            }
        }
        _ => unreachable!(), // Should only be called for &, |, !
    }
}

fn ext_test_word<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);
        // Collect a word that may consist of multiple adjacent segments:
        // quoted strings, bare characters, and expansions — without any
        // whitespace in between.  For example: "declare -a"* is one word
        // with a double-quoted segment followed by a bare glob character.
        let mut word = String::new();

        while let Ok(ch) = peek_char().parse_next(input) {
            match ch {
                // Single-quoted segment: capture with quotes
                '\'' => {
                    ext_test_parse_single_quoted(input, &mut word)?;
                }
                // Double-quoted segment: capture with quotes, handling escapes
                '"' => {
                    ext_test_parse_double_quoted(input, &mut word)?;
                }
                // Stop on whitespace
                ' ' | '\t' | '\n' => break,
                // Backslash escape: consume \ and next char
                '\\' => {
                    ext_test_parse_backslash_escape(input, &mut word)?;
                }
                // $ starts expansions that may contain parentheses
                '$' => {
                    ext_test_parse_dollar_expansion(input, &mut word)?;
                }
                // Stop on bare parentheses (used for grouping in [[ ]])
                '(' | ')' => break,
                // &, |, ! are special characters
                '&' | '|' | '!' => {
                    if !ext_test_parse_special_char(input, &mut word, ch)? {
                        break;
                    }
                }
                // Any other character is part of the word
                _ => {
                    word.push(ch);
                    winnow::token::any.parse_next(input)?;
                }
            }
        }

        if word.is_empty() {
            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
        } else {
            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);
            Ok(ast::Word {
                value: word,
                loc: Some(loc),
            })
        }
    }
}

/// Parse a regex word in extended test context (allows | ( ) [ ] in the pattern)
fn ext_test_regex_word<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);
        let mut result = String::new();

        loop {
            // Skip whitespace between parts
            if result.is_empty() {
                spaces().parse_next(input)?;
            }

            let checkpoint = input.checkpoint();

            // Check if we hit a stop condition (&&, ||, ]], or end)
            if winnow::combinator::opt::<_, _, PError, _>(winnow::combinator::alt((
                ("&", "&").map(|_| ()),
                ("|", "|").map(|_| ()),
                ("]", "]").map(|_| ()), // ]] stops the regex
            )))
            .parse_next(input)?
            .is_some()
            {
                input.reset(&checkpoint);
                break;
            }

            // Try to parse next component
            if let Ok(ch) = peek_char().parse_next(input) {
                match ch {
                    '\'' | '"' => {
                        // Quoted string
                        if !result.is_empty() {
                            result.push(' ');
                        }
                        let word = ext_test_word(tracker).parse_next(input)?;
                        result.push_str(&word.value);
                    }
                    '(' | ')' | '[' | ']' => {
                        // These are allowed in regex patterns
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    '|' if input.peek_token().is_some() => {
                        // Single | (not ||) is allowed in regex
                        let next_checkpoint = input.checkpoint();
                        winnow::token::any.parse_next(input)?;
                        if peek_char().parse_next(input).ok() == Some('|') {
                            // This is ||, backtrack
                            input.reset(&next_checkpoint);
                            break;
                        }
                        result.push('|');
                    }
                    ' ' | '\t' | '\n' if !result.is_empty() => {
                        // Stop on whitespace after we've collected something
                        break;
                    }
                    _ => {
                        // Regular word character
                        // Add space only if the last character was a regular word character
                        if !result.is_empty() {
                            let last_ch = result.chars().last();
                            // Don't add space after structural characters: ( ) [ ] |
                            if !matches!(last_ch, Some('(' | ')' | '[' | ']' | '|')) {
                                result.push(' ');
                            }
                        }
                        let word = take_while(1.., |c: char| {
                            !matches!(c, ' ' | '\t' | '\n' | '&' | '|' | '(' | ')' | '[' | ']')
                        })
                        .parse_next(input)?;
                        result.push_str(word);
                    }
                }
            } else {
                break;
            }
        }

        if result.is_empty() {
            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
        } else {
            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);
            Ok(ast::Word {
                value: result,
                loc: Some(loc),
            })
        }
    }
}

/// Parse primary extended test expression (parentheses, binary/unary tests, or word)
fn ext_test_primary<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExpr, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        ext_test_spaces().parse_next(input)?;

        // Try parenthesized expression
        if winnow::combinator::opt::<_, _, PError, _>('(')
            .parse_next(input)?
            .is_some()
        {
            ext_test_spaces().parse_next(input)?;
            let expr = ext_test_or_expr(tracker).parse_next(input)?;
            ext_test_spaces().parse_next(input)?;
            ')'.parse_next(input)?;
            return Ok(ast::ExtendedTestExpr::Parenthesized(Box::new(expr)));
        }

        // Try unary test (operator + operand)
        let checkpoint = input.checkpoint();
        if let Ok(op_word) = ext_test_word(tracker).parse_next(input) {
            if let Some(unary_pred) = parse_unary_operator(&op_word.value) {
                ext_test_spaces().parse_next(input)?;
                let operand = ext_test_word(tracker).parse_next(input)?;
                return Ok(ast::ExtendedTestExpr::UnaryTest(unary_pred, operand));
            }
        }
        input.reset(&checkpoint);

        // Try binary test (operand + operator + operand)
        let left_word = ext_test_word(tracker).parse_next(input)?;
        ext_test_spaces().parse_next(input)?;

        // Check for binary operator
        let checkpoint2 = input.checkpoint();
        if let Ok(op_word) = ext_test_word(tracker).parse_next(input) {
            if let Some(mut binary_pred) = parse_binary_operator(&op_word.value) {
                let is_regex_op = matches!(binary_pred, ast::BinaryPredicate::StringMatchesRegex);
                ext_test_spaces().parse_next(input)?;

                // For =~ operator, use regex word parser that allows | ( )
                let right_word = if is_regex_op {
                    ext_test_regex_word(tracker).parse_next(input)?
                } else {
                    ext_test_word(tracker).parse_next(input)?
                };

                // Special case: =~ with quoted string should use StringContainsSubstring
                if is_regex_op
                    && (right_word.value.starts_with('\'') || right_word.value.starts_with('"'))
                {
                    binary_pred = ast::BinaryPredicate::StringContainsSubstring;
                }

                return Ok(ast::ExtendedTestExpr::BinaryTest(
                    binary_pred,
                    left_word,
                    right_word,
                ));
            }
        }
        input.reset(&checkpoint2);

        // Fallback: single word tests for non-zero length
        Ok(ast::ExtendedTestExpr::UnaryTest(
            ast::UnaryPredicate::StringHasNonZeroLength,
            left_word,
        ))
    }
}

/// Parse NOT expression (right-associative)
fn ext_test_not_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExpr, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        ext_test_spaces().parse_next(input)?;

        // Check for NOT operator
        let checkpoint = input.checkpoint();
        if winnow::combinator::opt::<_, _, PError, _>('!')
            .parse_next(input)?
            .is_some()
        {
            // Make sure it's not != operator
            if peek_char().parse_next(input).ok() == Some('=') {
                input.reset(&checkpoint);
                return ext_test_primary(tracker).parse_next(input);
            }

            // Parse NOT recursively (right-associative)
            let expr = ext_test_not_expr(tracker).parse_next(input)?;
            return Ok(ast::ExtendedTestExpr::Not(Box::new(expr)));
        }

        ext_test_primary(tracker).parse_next(input)
    }
}

/// Parse AND expression (left-associative)
fn ext_test_and_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExpr, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let mut left = ext_test_not_expr(tracker).parse_next(input)?;

        loop {
            ext_test_spaces().parse_next(input)?;
            let checkpoint = input.checkpoint();

            // Check for && operator
            if winnow::combinator::opt::<_, _, PError, _>(("&", "&"))
                .parse_next(input)?
                .is_some()
            {
                let right = ext_test_not_expr(tracker).parse_next(input)?;
                left = ast::ExtendedTestExpr::And(Box::new(left), Box::new(right));
            } else {
                input.reset(&checkpoint);
                break;
            }
        }

        Ok(left)
    }
}

/// Parse OR expression (left-associative, lowest precedence)
fn ext_test_or_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExpr, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let mut left = ext_test_and_expr(tracker).parse_next(input)?;

        loop {
            ext_test_spaces().parse_next(input)?;
            let checkpoint = input.checkpoint();

            // Check for || operator
            if winnow::combinator::opt::<_, _, PError, _>(("|", "|"))
                .parse_next(input)?
                .is_some()
            {
                let right = ext_test_and_expr(tracker).parse_next(input)?;
                left = ast::ExtendedTestExpr::Or(Box::new(left), Box::new(right));
            } else {
                input.reset(&checkpoint);
                break;
            }
        }

        Ok(left)
    }
}

/// Parse extended test command: [[ expression ]]
/// Corresponds to: winnow.rs `extended_test_command()`
pub fn extended_test_command<'a>(
    _ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExprCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse [[
        '['.parse_next(input)?;
        '['.parse_next(input)?;

        ext_test_spaces().parse_next(input)?;

        // Parse the expression directly using winnow parsers
        let expr = ext_test_or_expr(tracker).parse_next(input)?;

        ext_test_spaces().parse_next(input)?;

        // Parse ]]
        ']'.parse_next(input)?;
        ']'.parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ExtendedTestExprCommand { expr, loc })
    }
}

// ============================================================================
// Tier 14: Function Definitions
// ============================================================================

/// Parse a compound command - tries all compound command types
/// Corresponds to: winnow.rs `compound_command()`
pub fn compound_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompoundCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::preceded(
            spaces(),
            dispatch! {peek_char();
                '{' => brace_group(ctx, tracker).map(ast::CompoundCommand::BraceGroup),
                '(' => paren_compound(ctx, tracker),  // Handles both (( )) arithmetic and ( ) subshell
                'f' => for_or_arithmetic_for(ctx, tracker),  // Handles both for (( )) and for name in
                'c' => case_clause(ctx, tracker).map(ast::CompoundCommand::CaseClause),
                'i' => if_clause(ctx, tracker).map(ast::CompoundCommand::IfClause),
                'w' => while_clause(ctx, tracker).map(ast::CompoundCommand::WhileClause),
                'u' => until_clause(ctx, tracker).map(ast::CompoundCommand::UntilClause),
                _ => fail,
            },
        )
        .parse_next(input)
    }
}

/// Parse function body (compound command with optional redirects)
/// Corresponds to: winnow.rs `function_body()`
fn function_body<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::FunctionBody, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let cmd = compound_command(ctx, tracker).parse_next(input)?;
        let redirects = winnow::combinator::opt(winnow::combinator::preceded(
            spaces(),
            redirect_list(ctx, tracker),
        ))
        .parse_next(input)?;

        Ok(ast::FunctionBody(cmd, redirects))
    }
}

/// Parse function definition
/// Corresponds to: winnow.rs `function_definition()`
pub fn function_definition<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::FunctionDefinition, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Try "function name () body" or "function name body" format
        let has_function_keyword = keyword("function").parse_next(input).is_ok();

        // Track location of the function name
        let fname_start = tracker.offset_from_locating(input);
        let func_name = fname().parse_next(input)?;
        let fname_end = tracker.offset_from_locating(input);

        // Function names cannot be reserved words (unless preceded by `function` keyword)
        if !has_function_keyword && is_reserved_word(&func_name) {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        // Parse optional ()
        spaces().parse_next(input)?;
        let has_parens = if winnow::combinator::opt::<_, _, PError, _>('(')
            .parse_next(input)?
            .is_some()
        {
            spaces().parse_next(input)?;
            ')'.parse_next(input)?;
            true
        } else {
            false
        };

        // Must have either "function" keyword or parens
        if !has_function_keyword && !has_parens {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        linebreak().parse_next(input)?;

        let body = function_body(ctx, tracker).parse_next(input)?;

        // Create the fname Word with location
        let fname_loc = tracker.range_to_span(fname_start..fname_end);
        let fname_word = ast::Word {
            value: func_name,
            loc: Some(fname_loc),
        };

        Ok(ast::FunctionDefinition {
            fname: fname_word,
            body,
        })
    }
}

// ============================================================================
// Tier 6: Complete Commands and Programs
// ============================================================================

/// Parse a complete command (and/or lists with separators)
/// Corresponds to: winnow.rs `complete_command()`
pub fn complete_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompleteCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Parse first and_or (required)
        let first_ao = and_or(ctx, tracker).parse_next(input)?;

        // Try to parse (separator + spaces + and_or) pairs
        let mut items: Vec<ast::CompoundListItem> = vec![];

        // Try to get separator after first and_or
        spaces().parse_next(input)?; // Consume spaces
        if let Ok(sep) = separator_op().parse_next(input) {
            spaces().parse_next(input)?; // Consume spaces after separator

            // First item has a separator
            items.push(ast::CompoundListItem(first_ao, sep));

            // Parse remaining (and_or, separator) pairs
            loop {
                // Try to parse next and_or
                let Ok(ao) = and_or(ctx, tracker).parse_next(input) else {
                    break;
                };

                // Try to get separator
                spaces().parse_next(input)?;
                if let Ok(sep) = separator_op().parse_next(input) {
                    spaces().parse_next(input)?;
                    items.push(ast::CompoundListItem(ao, sep));
                } else {
                    // No separator - this is the final and_or
                    items.push(ast::CompoundListItem(ao, ast::SeparatorOperator::Sequence));
                    break;
                }
            }
        } else {
            // No separator - just one and_or
            items.push(ast::CompoundListItem(
                first_ao,
                ast::SeparatorOperator::Sequence,
            ));
        }

        Ok(ast::CompoundList(items))
    }
}

/// Parse a newline-separated complete command continuation
/// Corresponds to: winnow.rs `complete_command_continuation()`
fn complete_command_continuation<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompleteCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::preceded(newline_list(), complete_command(ctx, tracker))
            .parse_next(input)
    }
}

/// Parse multiple complete commands separated by newlines
/// Corresponds to: winnow.rs `complete_commands()`
pub fn complete_commands<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::CompleteCommand>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            complete_command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(0.., complete_command_continuation(ctx, tracker)),
        )
            .map(
                |(first, rest): (ast::CompleteCommand, Vec<ast::CompleteCommand>)| {
                    let mut commands = Vec::with_capacity(1 + rest.len());
                    commands.push(first);
                    commands.extend(rest);
                    commands
                },
            )
            .parse_next(input)
    }
}

/// Parse a complete program
/// Corresponds to: winnow.rs `program()`
pub fn program<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Program, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        linebreak().parse_next(input)?;
        let complete_commands = winnow::combinator::opt(complete_commands(ctx, tracker))
            .parse_next(input)?
            .unwrap_or_default();
        linebreak().parse_next(input)?;
        // Consume any trailing whitespace/comment that isn't followed by a newline
        // (e.g., a comment at the end of a file without a trailing newline).
        let _: &str =
            winnow::token::take_while(0.., |c: char| c == ' ' || c == '\t').parse_next(input)?;
        winnow::combinator::opt(comment()).parse_next(input)?;
        winnow::combinator::eof.parse_next(input)?;
        Ok(ast::Program { complete_commands })
    }
}

/// Parse a shell program from a string with full source location tracking
///
/// This is the main entry point for parsing shell scripts using the `winnow_str` parser.
/// It creates a `PositionTracker` for efficient line/column lookup and parses the entire program.
///
/// # Arguments
/// * `input` - The shell script source code to parse
/// * `_options` - Parser options (not yet used by `winnow_str` parser)
/// * `_source_info` - Source file information (not yet used by `winnow_str` parser)
///
/// Note: The `winnow_str` parser currently doesn't implement extended globbing, tilde expansion,
/// or POSIX/SH mode differences, so the options parameter is accepted for API compatibility
/// but not used. Similarly, `source_info` is not yet used for error reporting.
///
/// # Example
/// ```ignore
/// use brush_parser::parser::winnow_str::parse_program;
/// use brush_parser::parser::{ParserOptions, SourceInfo};
///
/// let result = parse_program("echo hello", &ParserOptions::default(), &SourceInfo::default());
/// ```
pub fn parse_program(
    input: &str,
    options: &ParserOptions,
    source_info: &SourceInfo,
) -> Result<ast::Program, PError> {
    let pending_heredoc_trailing = std::cell::RefCell::new(None);
    let ctx = ParseContext {
        options,
        source_info,
        pending_heredoc_trailing: &pending_heredoc_trailing,
    };
    let tracker = PositionTracker::new(input);
    let mut stream = LocatingSlice::new(input);
    program(&ctx, &tracker).parse_next(&mut stream)
}
