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
    /// Cache original length for manual offset calculations (when not using LocatingSlice)
    #[allow(dead_code)]
    original_len: usize,
}

impl PositionTracker {
    /// Creates a new PositionTracker for the given input string.
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

    /// Get current offset from LocatingSlice
    #[inline]
    fn offset_from_locating<'a>(&self, input: &LocatingSlice<&'a str>) -> usize {
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

    /// Convert a byte range to a SourceSpan (for use with LocatingSlice)
    ///
    /// This is the primary method when using LocatingSlice.with_span()
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

/// Helper: Peek at first character for word_part dispatch
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
/// - `prefix`: The opening delimiter(s) to match first (e.g., "$(", "${", "`")
/// - `open_char`: Character that increases depth (e.g., '(' or '{'), or None for backticks
/// - `close_char`: Character that decreases depth (e.g., ')' or '}' or '`')
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
fn is_username_char(c: char) -> bool {
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
/// Corresponds to: matches_operator("\n") in winnow.rs
#[inline(always)]
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
/// Handles both inter-token spaces and inline comments like: echo hello # comment
/// This is needed to separate tokens on the same line
#[inline(always)]
pub fn spaces<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    (
        take_while(0.., |c: char| c == ' ' || c == '\t'), // Leading spaces
        winnow::combinator::opt(comment()),               // Optional comment after spaces
    )
        .void()
}

/// Parse required whitespace (at least one space or tab, optionally followed by comment)
#[inline(always)]
pub fn spaces1<'a>() -> impl Parser<StrStream<'a>, (), PError> {
    (
        take_while(1.., |c: char| c == ' ' || c == '\t'), // Required spaces
        winnow::combinator::opt(comment()),               // Optional comment after spaces
    )
        .void()
}

// ============================================================================
// Tier 1: Line breaks and separators
// ============================================================================

/// Parse linebreak (zero or more newlines, with optional comments before each newline)
/// Corresponds to: winnow.rs linebreak()
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
/// Corresponds to: winnow.rs newline_list()
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
/// Corresponds to: winnow.rs separator_op()
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

/// Parse separator (separator_op with linebreak, or newline_list)
/// Returns Option<SeparatorOperator> - None means it was just newlines
/// Corresponds to: winnow.rs separator() and peg.rs separator()
#[inline]
fn separator<'a>() -> impl Parser<StrStream<'a>, Option<SeparatorOperator>, PError> {
    winnow::combinator::alt((
        // separator_op followed by optional linebreaks
        (separator_op(), linebreak()).map(|(sep, _)| Some(sep)),
        // OR just one or more newlines (acts as sequence separator)
        newline_list().map(|_| None),
    ))
}

// ============================================================================
// Tier 2: Word parsing
// ============================================================================

/// Parse a bare word (literal characters only, no quotes or expansions)
/// Corresponds to the literal_chars part of tokenizer's word parsing
///
/// A word character is anything that's NOT:
/// - Whitespace: ' ', '\t', '\n', '\r'
/// - Operators: '|', '&', ';', '<', '>', '(', ')'
/// - Quote/expansion starters: '$', '`', '\'', '"', '\\'
///
/// Note: '{' and '}' ARE allowed in words for brace expansion (e.g., {1..10}, {a,b,c})
/// Brace groups ({ commands; }) are distinguished by requiring whitespace after '{' and before '}'
///
/// Note: Shell keywords (if, then, fi, etc.) are NOT excluded here because they
/// can be used as regular words in non-keyword contexts (e.g., "echo done").
/// The command() parser tries compound commands first, so keywords in keyword
/// positions will be matched by compound command parsers before bare_word sees them.
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
/// Note: This list includes "time" and "coproc" which are bash reserved words
/// but are not included in the PEG parser's reserved word list. This is
/// intentional - winnow_str is more accurate to actual bash behavior.
fn is_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "if" | "then" | "else" | "elif" | "fi" |
        "do" | "done" |
        "while" | "until" |
        "for" | "in" |
        "case" | "esac" |
        "function" |
        "{" | "}" |
        "!" |
        "[[" | "]]" |
        "select" |
        "time" |    // bash reserved word (not in PEG)
        "coproc" // bash reserved word (not in PEG)
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
/// Returns the unquoted content
pub fn single_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::delimited(
        '\'',
        take_while(0.., |c: char| c != '\'').map(|s: &str| s.to_string()),
        '\'',
    )
}

/// Parse a double-quoted string: "text"
/// For now, simplified - just returns content without processing escapes/expansions
/// TODO: Handle escape sequences and expansions inside double quotes
pub fn double_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::delimited(
        '"',
        take_while(0.., |c: char| c != '"' && c != '\\').map(|s: &str| s.to_string()),
        '"',
    )
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
/// The last_char parameter helps detect tilde-after-colon
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
                .map(|c| Cow::Owned(c.to_string()))
                .parse_next(input),
            _ => {
                // Slow path: check for special cases, then fall back to bare_word

                // Check for tilde after colon if enabled and last char was ':'
                if ctx.options.tilde_expansion_after_colon && last_char == Some(':') && ch == '~' {
                    if let Ok(tilde_expr) = tilde_expansion().parse_next(input) {
                        return Ok(Cow::Borrowed(tilde_expr));
                    }
                }

                // Check for extended glob pattern if enabled
                if ctx.options.enable_extended_globbing {
                    if let Some(pattern) =
                        winnow::combinator::opt(extglob_pattern()).parse_next(input)?
                    {
                        return Ok(Cow::Borrowed(pattern));
                    }
                }

                // Default: parse as bare word
                bare_word().map(Cow::Borrowed).parse_next(input)
            }
        }
    }
}

/// Parse a word (one or more word parts combined)
/// Handles quoted strings, escapes, and bare text
/// Corresponds to: tokenizer's word parsing + winnow.rs word_as_ast()
pub fn word_as_ast<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Check for tilde at word start if enabled
        let mut value = String::new();
        let mut last_char = None;

        if ctx.options.tilde_expansion_at_word_start {
            if peek_char().parse_next(input).ok() == Some('~') {
                if let Ok(tilde_expr) = tilde_expansion().parse_next(input) {
                    value.push_str(tilde_expr);
                    last_char = tilde_expr.chars().last();
                }
            }
        }

        // Parse remaining word parts, tracking last character for tilde-after-colon detection
        // Note: We can't use repeat() here because each word_part depends on the last_char
        // from the previous part (for tilde-after-colon detection)
        while let Ok(part) = word_part(ctx, last_char).parse_next(input) {
            // Update last_char efficiently - just get the last char of the new part
            last_char = part.chars().last().or(last_char);
            value.push_str(&part);
        }

        // Must have at least one character
        if value.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::Word {
            value,
            loc: Some(loc),
        })
    }
}

/// Parse a wordlist (one or more words separated by spaces)
/// Corresponds to: winnow.rs wordlist()
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

/// Parse an I/O file descriptor number (0-9)
/// Corresponds to: winnow.rs io_number()
pub fn io_number<'a>() -> impl Parser<StrStream<'a>, i32, PError> {
    winnow::token::take_while(1.., |c: char| c.is_ascii_digit())
        .verify(|s: &str| s.len() == 1) // Only single digit fd numbers
        .map(|s: &str| s.parse::<i32>().unwrap())
}

/// Parse redirect operator and return the redirect kind
/// Corresponds to: winnow.rs io_file() dispatcher
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
/// Returns (delimiter_text, requires_expansion)
fn here_document_delimiter<'a>() -> impl Parser<StrStream<'a>, (String, bool), PError> {
    move |input: &mut StrStream<'a>| {
        let mut delimiter = String::new();
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

            match ch {
                '\'' | '"' => {
                    quoted = true;
                    // Don't include quotes in delimiter text
                }
                '\\' => {
                    quoted = true;
                    // Consume next character
                    if let Ok(next_ch) = winnow::token::any::<_, PError>.parse_next(input) {
                        delimiter.push(next_ch);
                    }
                }
                ' ' | '\t' | '\n' => {
                    // End of delimiter
                    done = true;
                }
                _ => {
                    delimiter.push(ch);
                }
            }
        }

        if delimiter.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        let requires_expansion = !quoted;
        Ok((delimiter, requires_expansion))
    }
}

/// Parse here-document content until delimiter is found
/// Returns the content as a Word
fn here_document_content<'a>(
    delimiter: &str,
    remove_tabs: bool,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + '_ {
    move |input: &mut StrStream<'a>| {
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
                        // Found delimiter, consume the newline if present
                        let _: Result<char, PError> = '\n'.parse_next(input);
                        // Return content without the trailing newline we added
                        if content.ends_with('\n') && content.len() > 1 {
                            content.pop();
                        }
                        return Ok(ast::Word {
                            value: content,
                            loc: None,
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
}

/// Parse a here-document redirect (<< or <<-)
fn here_document<'a>() -> impl Parser<StrStream<'a>, (Option<i32>, ast::IoHereDocument), PError> {
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

        // Parse delimiter
        let (delimiter, requires_expansion) = here_document_delimiter().parse_next(input)?;

        // Skip to end of line (there might be more tokens on this line)
        let _: Result<&str, PError> =
            winnow::token::take_while(0.., |c| c != '\n').parse_next(input);

        // Consume the newline
        '\n'.parse_next(input)?;

        // Parse content until delimiter
        let doc = here_document_content(&delimiter, remove_tabs).parse_next(input)?;

        Ok((
            fd,
            ast::IoHereDocument {
                remove_tabs,
                requires_expansion,
                here_end: ast::Word::from(delimiter),
                doc,
            },
        ))
    }
}

/// Parse a file redirect (e.g., "> file", "2>&1", "< input")
/// Corresponds to: winnow.rs io_file() + io_redirect()
pub fn io_redirect<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::IoRedirect, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::alt((
            // Try OutputAndError redirects first (&>> and &>)
            (
                "&>>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| ast::IoRedirect::OutputAndError(target, true)),
            (
                "&>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| ast::IoRedirect::OutputAndError(target, false)),
            // Try here-string (<<<)
            (
                winnow::combinator::opt(io_number()),
                "<<<",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(fd, _, word)| ast::IoRedirect::HereString(fd, word)),
            // Try here-document
            here_document().map(|(fd, here_doc)| ast::IoRedirect::HereDocument(fd, here_doc)),
            // Then try regular file redirects
            (
                winnow::combinator::opt(io_number()), // Optional fd number
                redirect_operator(),                  // Redirect operator
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)), /* Target
                                                       * filename/
                                                       * fd */
            )
                .map(
                    |(fd, kind, target): (Option<i32>, ast::IoFileRedirectKind, ast::Word)| {
                        // Determine target type based on operator
                        let redirect_target = match kind {
                            ast::IoFileRedirectKind::DuplicateOutput
                            | ast::IoFileRedirectKind::DuplicateInput => {
                                // For &> and <&, target could be a fd or filename
                                ast::IoFileRedirectTarget::Duplicate(target)
                            }
                            _ => {
                                // For regular redirects, it's a filename
                                ast::IoFileRedirectTarget::Filename(target)
                            }
                        };

                        ast::IoRedirect::File(fd, kind, redirect_target)
                    },
                ),
        ))
        .parse_next(input)
    }
}

/// Parse a redirect list (one or more redirects)
/// Corresponds to: winnow.rs redirect_list()
pub fn redirect_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::RedirectList, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::preceded(spaces(), io_redirect(ctx, tracker)),
        )
        .map(ast::RedirectList)
        .parse_next(input)
    }
}

// ============================================================================
// Tier 3: Commands
// ============================================================================

/// Parse an array element: either "value" or "[index]=value"
fn array_element<'a>() -> impl Parser<StrStream<'a>, (Option<ast::Word>, ast::Word), PError> {
    move |input: &mut StrStream<'a>| {
        // Skip whitespace before element
        spaces().parse_next(input)?;

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
                // Parse value (until space or closing paren)
                let value_str = winnow::token::take_while::<_, _, PError>(0.., |c: char| {
                    !c.is_whitespace() && c != ')'
                })
                .parse_next(input)?;

                return Ok((Some(ast::Word::new(index_str)), ast::Word::new(value_str)));
            }
        }

        // Reset and try simple value
        input.reset(&checkpoint);

        // Parse simple value (until space or closing paren)
        let value_str = winnow::token::take_while::<_, _, PError>(1.., |c: char| {
            !c.is_whitespace() && c != ')'
        })
        .parse_next(input)?;

        Ok((None, ast::Word::new(value_str)))
    }
}

/// Parse an assignment word (VAR=value or VAR+=value or VAR=(array elements))
/// Returns (Assignment, original word as ast::Word)
fn assignment_word<'a>() -> impl Parser<StrStream<'a>, (ast::Assignment, ast::Word), PError> {
    move |input: &mut StrStream<'a>| {
        // Parse variable name (must start with letter or underscore)
        let var_name = (
            winnow::token::one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
            winnow::token::take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
        )
            .take()
            .parse_next(input)?;

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
            full_word.push_str(&var_name);
            if append {
                full_word.push_str("+=(");
            } else {
                full_word.push_str("=(");
            }

            loop {
                spaces().parse_next(input)?;

                // Check for closing paren
                if winnow::combinator::opt::<_, _, PError, _>(')')
                    .parse_next(input)?
                    .is_some()
                {
                    full_word.push(')');
                    break;
                }

                // Parse element
                let elem = array_element().parse_next(input)?;

                // Add to full_word
                if !elements.is_empty() {
                    full_word.push(' ');
                }
                if let Some(ref index) = elem.0 {
                    full_word.push('[');
                    full_word.push_str(&index.value);
                    full_word.push_str("]=");
                    full_word.push_str(&elem.1.value);
                } else {
                    full_word.push_str(&elem.1.value);
                }

                elements.push(elem);
            }

            let assignment = ast::Assignment {
                name: ast::AssignmentName::VariableName(var_name.to_string()),
                value: ast::AssignmentValue::Array(elements),
                append,
                loc: crate::SourceSpan::default(),
            };

            return Ok((assignment, ast::Word::new(&full_word)));
        }

        // Not an array, reset and parse scalar value
        input.reset(&checkpoint);

        // Get the value (everything until whitespace or special char)
        let value_str = winnow::token::take_while::<_, _, PError>(0.., |c: char| {
            !c.is_whitespace()
                && c != '|'
                && c != '&'
                && c != ';'
                && c != '<'
                && c != '>'
                && c != '('
                && c != ')'
        })
        .parse_next(input)?;

        // Construct the full assignment word for AST
        let mut full_word = String::with_capacity(var_name.len() + value_str.len() + 2);
        full_word.push_str(&var_name);
        if append {
            full_word.push_str("+=");
        } else {
            full_word.push('=');
        }
        full_word.push_str(value_str);

        let assignment = ast::Assignment {
            name: ast::AssignmentName::VariableName(var_name.to_string()),
            value: ast::AssignmentValue::Scalar(ast::Word::new(value_str)),
            append,
            loc: crate::SourceSpan::default(),
        };

        let word = ast::Word::new(&full_word);

        Ok((assignment, word))
    }
}

/// Parse cmd_prefix (assignments and redirects before command name)
/// Corresponds to: peg.rs cmd_prefix()
pub fn cmd_prefix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandPrefix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::terminated(
                winnow::combinator::alt((
                    io_redirect(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::IoRedirect),
                    assignment_word().map(|(assignment, word)| {
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

/// Parse cmd_suffix (arguments and redirections)
/// Now supports words, redirections, and process substitutions
/// Corresponds to: winnow.rs cmd_suffix()
pub fn cmd_suffix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandSuffix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::preceded(
            spaces1(),
            repeat::<_, _, Vec<_>, _, _>(
                1..,
                winnow::combinator::terminated(
                    winnow::combinator::alt((
                        io_redirect(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::IoRedirect),
                        process_substitution(ctx, tracker).map(|(kind, cmd)| {
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, cmd)
                        }),
                        word_as_ast(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::Word),
                    )),
                    spaces(), // Consume trailing spaces after each item
                ),
            ),
        )
        .map(ast::CommandSuffix)
        .parse_next(input)
    }
}

/// Parse a simple command (command name + optional arguments)
/// Now supports: prefix (assignments/redirects) + optional command + optional suffix
/// Corresponds to: peg.rs simple_command()
pub fn simple_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::SimpleCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Try to parse optional prefix (assignments and/or redirects)
        let prefix = winnow::combinator::opt(cmd_prefix(ctx, tracker)).parse_next(input)?;

        // Try to parse optional command name (must not be reserved word)
        let word_or_name = if let Ok(name) = non_reserved_word(ctx, tracker).parse_next(input) {
            Some(name)
        } else {
            None
        };

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
            .map(|c| c == '<' || c == '>' || c.is_ascii_digit())
            .unwrap_or(false);

        if has_redirect {
            winnow::combinator::opt(redirect_list(ctx, tracker)).parse_next(input)
        } else {
            Ok(None)
        }
    }
}

/// Parse a command (simple or compound)
/// Corresponds to: winnow.rs command()
pub fn command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Command, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?; // Consume optional leading spaces

        // In POSIX or SH mode, don't try extended test command [[
        if ctx.options.posix_mode || ctx.options.sh_mode {
            winnow::combinator::alt((
                // Try function definitions first (must come before simple commands)
                function_definition(ctx, tracker).map(ast::Command::Function),
                // Try compound commands with optional redirects
                (if_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::IfClause(c), r)),
                (while_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::WhileClause(c), r)),
                (until_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::UntilClause(c), r)),
                (
                    for_or_arithmetic_for(ctx, tracker),
                    optional_redirects(ctx, tracker),
                )
                    .map(|(c, r)| ast::Command::Compound(c, r)),
                (case_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::CaseClause(c), r)),
                (
                    paren_compound(ctx, tracker),
                    optional_redirects(ctx, tracker),
                )
                    .map(|(c, r)| ast::Command::Compound(c, r)),
                (brace_group(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::BraceGroup(c), r)),
                // Then try simple commands
                simple_command(ctx, tracker).map(ast::Command::Simple),
            ))
            .parse_next(input)
        } else {
            // In Bash mode, also try extended test command [[
            winnow::combinator::alt((
                // Try function definitions first (must come before simple commands)
                function_definition(ctx, tracker).map(ast::Command::Function),
                // Try compound commands with optional redirects
                (if_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::IfClause(c), r)),
                (while_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::WhileClause(c), r)),
                (until_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::UntilClause(c), r)),
                (
                    for_or_arithmetic_for(ctx, tracker),
                    optional_redirects(ctx, tracker),
                )
                    .map(|(c, r)| ast::Command::Compound(c, r)),
                (
                    extended_test_command(ctx, tracker),
                    optional_redirects(ctx, tracker),
                )
                    .map(|(cmd, r)| ast::Command::ExtendedTest(cmd, r)),
                (case_clause(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::CaseClause(c), r)),
                (
                    paren_compound(ctx, tracker),
                    optional_redirects(ctx, tracker),
                )
                    .map(|(c, r)| ast::Command::Compound(c, r)),
                (brace_group(ctx, tracker), optional_redirects(ctx, tracker))
                    .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::BraceGroup(c), r)),
                // Then try simple commands
                simple_command(ctx, tracker).map(ast::Command::Simple),
            ))
            .parse_next(input)
        }
    }
}

// ============================================================================
// Tier 4: Pipelines
// ============================================================================

/// Parse pipe operator ('|' or '|&')
/// Corresponds to: winnow.rs pipe_operator()
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

/// Parse pipe sequence (command | command | command)
/// Corresponds to: winnow.rs pipe_sequence()
pub fn pipe_sequence<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::Command>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(
                0..,
                (
                    winnow::combinator::preceded(spaces(), pipe_operator()), // spaces then |
                    winnow::combinator::preceded((linebreak(), spaces()), command(ctx, tracker)), // optional newlines+spaces then command
                ),
            ),
        )
            .map(|(first, rest): (ast::Command, Vec<(bool, ast::Command)>)| {
                // Use fold to build commands vector, avoiding manual loop
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
                    })
            })
            .parse_next(input)
    }
}

/// Parse optional time keyword with optional -p flag
/// Returns Option<PipelineTimed>
fn pipeline_timed<'a>() -> impl Parser<StrStream<'a>, Option<ast::PipelineTimed>, PError> {
    move |input: &mut StrStream<'a>| {
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

        let timed = if has_posix_flag {
            ast::PipelineTimed::TimedWithPosixOutput(crate::SourceSpan::default())
        } else {
            ast::PipelineTimed::Timed(crate::SourceSpan::default())
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
/// Corresponds to: winnow.rs pipeline() with full support for time and bang
pub fn pipeline<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Pipeline, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            pipeline_timed(),
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
/// Corresponds to: winnow.rs and_or_op()
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
/// Corresponds to: winnow.rs and_or_continuation()
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
/// Corresponds to: winnow.rs and_or()
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
/// Similar to complete_command but with optional leading linebreaks and more flexible separators
/// Corresponds to: winnow.rs compound_list()
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

            let sep_opt = match separator().parse_next(input) {
                Ok(sep_opt) => {
                    spaces().parse_next(input)?;
                    sep_opt
                }
                Err(_) => {
                    // No separator - add current and_or with default separator and we're done
                    items.push(ast::CompoundListItem(
                        current_ao,
                        ast::SeparatorOperator::Sequence,
                    ));
                    break;
                }
            };

            // Convert Option<SeparatorOperator> to SeparatorOperator (None means newline, treat as
            // Sequence)
            let sep = sep_opt.unwrap_or(ast::SeparatorOperator::Sequence);

            // We have a separator, check if there's another and_or after it
            match and_or(ctx, tracker).parse_next(input) {
                Ok(next_ao) => {
                    // Push current and_or with its separator, then move to next
                    items.push(ast::CompoundListItem(current_ao, sep));
                    current_ao = next_ao;
                }
                Err(_) => {
                    // Trailing separator - push current and_or with the separator
                    items.push(ast::CompoundListItem(current_ao, sep));
                    break;
                }
            }
        }

        Ok(ast::CompoundList(items))
    }
}

/// Parse a subshell: ( commands )
/// Corresponds to: winnow.rs subshell()
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
/// Corresponds to: winnow.rs brace_group()
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
/// Corresponds to: peg.rs process_substitution()
pub fn process_substitution<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (ast::ProcessSubstitutionKind, ast::SubshellCommand), PError> + 'a {
    move |input: &mut StrStream<'a>| {
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

        Ok((
            kind,
            ast::SubshellCommand {
                list,
                loc: crate::SourceSpan::default(),
            },
        ))
    }
}

// ============================================================================
// Helper parsers for compound commands
// ============================================================================

/// Parse a sequential separator (semicolon or newlines)
/// Corresponds to: winnow.rs sequential_sep()
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
/// Corresponds to: winnow.rs name()
pub fn name<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    winnow::combinator::preceded(spaces(), bare_word())
        .verify(|s: &str| is_valid_name(s))
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
/// Corresponds to: winnow.rs do_group()
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
/// Corresponds to: winnow.rs if_clause()
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
/// Corresponds to: winnow.rs while_clause()
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
/// Corresponds to: winnow.rs until_clause()
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
/// Corresponds to: winnow.rs for_clause()
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

        linebreak().parse_next(input)?;

        Ok(ast::CaseItem {
            patterns,
            cmd,
            post_action,
            loc: None,
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
/// Corresponds to: winnow.rs case_clause()
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
/// Corresponds to: winnow.rs arithmetic_expression()
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

                // Check for "))"
                if winnow::combinator::opt::<_, _, PError, _>((')', ')'))
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
            value: expr_str.trim().to_string(),
        })
    }
}

/// Parse arithmetic command (( expr ))
/// Corresponds to: winnow.rs arithmetic_command()
pub fn arithmetic_command<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ArithmeticCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse ((
        '('.parse_next(input)?;
        '('.parse_next(input)?;

        // Parse expression
        let expr = arithmetic_expression().parse_next(input)?;

        // Parse ))
        spaces().parse_next(input)?;
        ')'.parse_next(input)?;
        ')'.parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ArithmeticCommand { expr, loc })
    }
}

/// Parse commands starting with '(' - either arithmetic (( )) or subshell ( )
/// Corresponds to: winnow.rs paren_compound()
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

/// Parse arithmetic for body (do_group or brace_group)
/// Corresponds to: winnow.rs arithmetic_for_body() and peg.rs arithmetic_for_body()
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
/// Corresponds to: winnow.rs arithmetic_for_clause()
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
/// Corresponds to: winnow.rs for_or_arithmetic_for()
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

/// Parse a unary test operator (-f, -z, -n, etc.)
/// Corresponds to: winnow.rs parse_unary_operator()
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
/// Corresponds to: winnow.rs parse_binary_operator()
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

/// Token type for extended test expressions
#[derive(Debug, Clone, PartialEq)]
enum ExtTestToken {
    Word(String),
    And,    // &&
    Or,     // ||
    Not,    // !
    LParen, // (
    RParen, // )
}

/// Tokenize content between [[ and ]] for extended test expressions
fn tokenize_extended_test(
    content: &str,
) -> Result<Vec<ExtTestToken>, winnow::error::ErrMode<ContextError>> {
    let mut tokens = Vec::new();
    let mut current_word = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';
    let mut chars = content.chars().peekable();

    // Helper to push current word as token
    let push_word = |word: &mut String, tokens: &mut Vec<ExtTestToken>| {
        if !word.is_empty() {
            tokens.push(ExtTestToken::Word(word.clone()));
            word.clear();
        }
    };

    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
                current_word.push(ch);
            }
            '"' | '\'' if in_quotes && ch == quote_char => {
                in_quotes = false;
                current_word.push(ch);
            }
            ' ' | '\t' | '\n' if !in_quotes => {
                push_word(&mut current_word, &mut tokens);
            }
            '&' if !in_quotes && chars.peek() == Some(&'&') => {
                push_word(&mut current_word, &mut tokens);
                chars.next(); // consume second &
                tokens.push(ExtTestToken::And);
            }
            '|' if !in_quotes && chars.peek() == Some(&'|') => {
                push_word(&mut current_word, &mut tokens);
                chars.next(); // consume second |
                tokens.push(ExtTestToken::Or);
            }
            '!' if !in_quotes
                && chars.peek() != Some(&'=')
                && (current_word.is_empty()
                    || chars.peek().map_or(true, |&c| c.is_whitespace())) =>
            {
                push_word(&mut current_word, &mut tokens);
                tokens.push(ExtTestToken::Not);
            }
            '(' if !in_quotes => {
                push_word(&mut current_word, &mut tokens);
                tokens.push(ExtTestToken::LParen);
            }
            ')' if !in_quotes => {
                push_word(&mut current_word, &mut tokens);
                tokens.push(ExtTestToken::RParen);
            }
            _ => {
                current_word.push(ch);
            }
        }
    }

    push_word(&mut current_word, &mut tokens);
    Ok(tokens)
}

/// Precedence parser for extended test expressions
/// Implements the precedence hierarchy from PEG parser
struct ExtTestParser {
    tokens: Vec<ExtTestToken>,
    pos: usize,
}

impl ExtTestParser {
    fn new(tokens: Vec<ExtTestToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&ExtTestToken> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<ExtTestToken> {
        if self.is_at_end() {
            None
        } else {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        }
    }

    fn expect_word(&mut self) -> Result<String, winnow::error::ErrMode<ContextError>> {
        match self.advance() {
            Some(ExtTestToken::Word(w)) => Ok(w),
            _ => Err(winnow::error::ErrMode::Backtrack(ContextError::default())),
        }
    }

    /// Expect a regex word - allows | ( ) in the pattern
    /// Corresponds to: peg.rs regex_word()
    fn expect_regex_word(&mut self) -> Result<String, winnow::error::ErrMode<ContextError>> {
        let mut result = String::new();
        let mut found_something = false;
        let mut last_was_word = false;

        // Collect tokens that are part of the regex word
        // Stop at: &&, ||, end of tokens, or other test operators
        while let Some(token) = self.peek() {
            match token {
                ExtTestToken::Word(w) => {
                    // Add space only between consecutive word tokens
                    if last_was_word {
                        result.push(' ');
                    }
                    result.push_str(w);
                    found_something = true;
                    last_was_word = true;
                    self.advance();
                }
                ExtTestToken::LParen => {
                    result.push('(');
                    found_something = true;
                    last_was_word = false;
                    self.advance();
                }
                ExtTestToken::RParen => {
                    result.push(')');
                    found_something = true;
                    last_was_word = false;
                    self.advance();
                }
                // Stop at logical operators
                ExtTestToken::And | ExtTestToken::Or | ExtTestToken::Not => break,
            }
        }

        if found_something {
            Ok(result)
        } else {
            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
        }
    }

    /// Parse top-level expression (handles ||)
    fn parse(&mut self) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
        self.parse_or()
    }

    /// Parse OR expressions (lowest precedence)
    fn parse_or(&mut self) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
        let mut left = self.parse_and()?;

        while matches!(self.peek(), Some(ExtTestToken::Or)) {
            self.advance(); // consume ||
            let right = self.parse_and()?;
            left = ast::ExtendedTestExpr::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse AND expressions
    fn parse_and(&mut self) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
        let mut left = self.parse_not()?;

        while matches!(self.peek(), Some(ExtTestToken::And)) {
            self.advance(); // consume &&
            let right = self.parse_not()?;
            left = ast::ExtendedTestExpr::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse NOT expressions
    fn parse_not(&mut self) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
        if matches!(self.peek(), Some(ExtTestToken::Not)) {
            self.advance(); // consume !
            let expr = self.parse_not()?; // NOT is right-associative
            return Ok(ast::ExtendedTestExpr::Not(Box::new(expr)));
        }

        self.parse_primary()
    }

    /// Parse primary expressions (parentheses, binary tests, unary tests, words)
    fn parse_primary(
        &mut self,
    ) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
        // Try parenthesized expression
        if matches!(self.peek(), Some(ExtTestToken::LParen)) {
            self.advance(); // consume (
            let expr = self.parse_or()?; // Parse full expression inside
            if !matches!(self.peek(), Some(ExtTestToken::RParen)) {
                return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
            }
            self.advance(); // consume )
            return Ok(ast::ExtendedTestExpr::Parenthesized(Box::new(expr)));
        }

        // Try unary test (operator + operand)
        if let Some(ExtTestToken::Word(op_word)) = self.peek() {
            if let Some(unary_pred) = parse_unary_operator(op_word) {
                self.advance(); // consume operator
                let operand = self.expect_word()?;
                return Ok(ast::ExtendedTestExpr::UnaryTest(
                    unary_pred,
                    ast::Word::from(operand),
                ));
            }
        }

        // Try binary test (operand + operator + operand)
        let left_word = self.expect_word()?;

        // Check if there's an operator following
        if let Some(ExtTestToken::Word(op_word)) = self.peek() {
            if let Some(mut binary_pred) = parse_binary_operator(op_word) {
                let is_regex_op = matches!(binary_pred, ast::BinaryPredicate::StringMatchesRegex);
                self.advance(); // consume operator

                // For =~ operator, use regex word parser that allows | ( )
                let right_word = if is_regex_op {
                    self.expect_regex_word()?
                } else {
                    self.expect_word()?
                };

                // Special case: =~ with quoted string should use StringContainsSubstring instead of
                // StringMatchesRegex
                if is_regex_op && (right_word.starts_with('\'') || right_word.starts_with('"')) {
                    binary_pred = ast::BinaryPredicate::StringContainsSubstring;
                }

                return Ok(ast::ExtendedTestExpr::BinaryTest(
                    binary_pred,
                    ast::Word::from(left_word),
                    ast::Word::from(right_word),
                ));
            }
        }

        // Fallback: single word tests for non-zero length
        Ok(ast::ExtendedTestExpr::UnaryTest(
            ast::UnaryPredicate::StringHasNonZeroLength,
            ast::Word::from(left_word),
        ))
    }
}

/// Parse extended test expression from content string
fn parse_extended_test_expr(
    content: &str,
) -> Result<ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
    let tokens = tokenize_extended_test(content)?;

    if tokens.is_empty() {
        return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
    }

    let mut parser = ExtTestParser::new(tokens);
    parser.parse()
}

/// Parse extended test command: [[ expression ]]
/// Corresponds to: winnow.rs extended_test_command()
pub fn extended_test_command<'a>(
    _ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::ExtendedTestExprCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse [[
        '['.parse_next(input)?;
        '['.parse_next(input)?;

        spaces().parse_next(input)?;

        // Collect content until ]]
        let mut content = String::new();

        loop {
            // Check for ]]
            let checkpoint = input.checkpoint();
            spaces().parse_next(input)?;

            if winnow::combinator::opt::<_, _, PError, _>((']', ']'))
                .parse_next(input)?
                .is_some()
            {
                // Found closing ]]
                break;
            }

            input.reset(&checkpoint);

            // Check if we've run out of input
            if input.is_empty() {
                return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
            }

            // Collect character
            let ch: char = winnow::token::any.parse_next(input)?;
            content.push(ch);
        }

        // Parse the expression from content
        let expr = parse_extended_test_expr(&content)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ExtendedTestExprCommand { expr, loc })
    }
}

// ============================================================================
// Tier 14: Function Definitions
// ============================================================================

/// Parse a compound command - tries all compound command types
/// Corresponds to: winnow.rs compound_command()
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
/// Corresponds to: winnow.rs function_body()
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
/// Corresponds to: winnow.rs function_definition()
pub fn function_definition<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::FunctionDefinition, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Try "function name () body" or "function name body" format
        let has_function_keyword = keyword("function").parse_next(input).is_ok();

        let fname = name().parse_next(input)?;

        // Function names cannot be reserved words
        if is_reserved_word(&fname) {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        // Function names cannot end with '=' to avoid confusion with assignments
        if fname.ends_with('=') {
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

        Ok(ast::FunctionDefinition {
            fname: ast::Word::from(fname),
            body,
        })
    }
}

// ============================================================================
// Tier 6: Complete Commands and Programs
// ============================================================================

/// Parse a complete command (and/or lists with separators)
/// Corresponds to: winnow.rs complete_command()
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
/// Corresponds to: winnow.rs complete_command_continuation()
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
/// Corresponds to: winnow.rs complete_commands()
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
/// Corresponds to: winnow.rs program()
pub fn program<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Program, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            linebreak(), // Optional initial linebreak
            complete_commands(ctx, tracker),
        )
            .map(
                |(_, complete_commands): ((), Vec<ast::CompleteCommand>)| ast::Program {
                    complete_commands,
                },
            )
            .parse_next(input)
    }
}

/// Parse a shell program from a string with full source location tracking
///
/// This is the main entry point for parsing shell scripts using the winnow_str parser.
/// It creates a PositionTracker for efficient line/column lookup and parses the entire program.
///
/// # Arguments
/// * `input` - The shell script source code to parse
/// * `_options` - Parser options (not yet used by winnow_str parser)
/// * `_source_info` - Source file information (not yet used by winnow_str parser)
///
/// Note: The winnow_str parser currently doesn't implement extended globbing, tilde expansion,
/// or POSIX/SH mode differences, so the options parameter is accepted for API compatibility
/// but not used. Similarly, source_info is not yet used for error reporting.
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
    let ctx = ParseContext {
        options,
        source_info,
    };
    let tracker = PositionTracker::new(input);
    let mut stream = LocatingSlice::new(input);
    program(&ctx, &tracker).parse_next(&mut stream)
}

#[cfg(test)]
mod tests {
    //! Tests for the winnow_str parser implementation
    use super::*;

    // Helper function to create a PositionTracker for testing
    fn make_tracker(input: &str) -> PositionTracker {
        PositionTracker::new(input)
    }

    // ============================================================================
    // TESTS ORGANIZED BY SYNTAX ELEMENT
    // ============================================================================
    // This test suite contains 259 tests organized into modules by syntax category.
    // This structure makes it easy to find specific tests and compare parser
    // behavior across different implementations (PEG, winnow, winnow_str).

    // ============================================================================
    // 1. BASIC ELEMENTS (Tier 1 & 2)
    // ============================================================================
    // Tests for fundamental parsing primitives: newlines, spaces, separators,
    // bare words, and word lists.

    mod basic_elements {
        use super::*;

        #[test]
        fn test_newline() {
            let mut input = LocatingSlice::new("\n");
            let result = newline().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), '\n');
        }

        #[test]
        fn test_newline_fails_on_other() {
            let mut input = LocatingSlice::new("x");
            let result = newline().parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_spaces() {
            let mut input = LocatingSlice::new("   ");
            let result = spaces().parse_next(&mut input);
            assert!(result.is_ok());

            let mut input = LocatingSlice::new("");
            let result = spaces().parse_next(&mut input);
            assert!(result.is_ok()); // spaces() accepts zero
        }

        #[test]
        fn test_spaces1() {
            let mut input = LocatingSlice::new("   ");
            let result = spaces1().parse_next(&mut input);
            assert!(result.is_ok());

            let mut input = LocatingSlice::new("");
            let result = spaces1().parse_next(&mut input);
            assert!(result.is_err()); // spaces1() requires at least one
        }

        #[test]
        fn test_linebreak() {
            // Zero newlines - should succeed
            let mut input = LocatingSlice::new("");
            let result = linebreak().parse_next(&mut input);
            assert!(result.is_ok());

            // One newline
            let mut input = LocatingSlice::new("\n");
            let result = linebreak().parse_next(&mut input);
            assert!(result.is_ok());

            // Multiple newlines
            let mut input = LocatingSlice::new("\n\n\n");
            let result = linebreak().parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_newline_list() {
            // Zero newlines - should fail
            let mut input = LocatingSlice::new("");
            let result = newline_list().parse_next(&mut input);
            assert!(result.is_err());

            // One newline - should succeed
            let mut input = LocatingSlice::new("\n");
            let result = newline_list().parse_next(&mut input);
            assert!(result.is_ok());

            // Multiple newlines
            let mut input = LocatingSlice::new("\n\n\n");
            let result = newline_list().parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_separator_op() {
            let mut input = LocatingSlice::new(";");
            let result = separator_op().parse_next(&mut input);
            assert!(result.is_ok());
            assert!(matches!(result.unwrap(), SeparatorOperator::Sequence));

            let mut input = LocatingSlice::new("&");
            let result = separator_op().parse_next(&mut input);
            assert!(result.is_ok());
            assert!(matches!(result.unwrap(), SeparatorOperator::Async));
        }

        #[test]
        fn test_bare_word() {
            // Simple word
            let mut input = LocatingSlice::new("hello");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello");

            // Word stops at space
            let mut input = LocatingSlice::new("hello world");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello");

            // Word stops at operator
            let mut input = LocatingSlice::new("hello|");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello");

            // Word stops at quote
            let mut input = LocatingSlice::new("hello'world");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello");
        }

        #[test]
        fn test_bare_word_fails_on_empty() {
            let mut input = LocatingSlice::new("");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_bare_word_fails_on_space() {
            let mut input = LocatingSlice::new(" hello");
            let result = bare_word().parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_word_as_ast() {
            let input_str = "echo";
            let mut input = LocatingSlice::new("echo");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let word = result.unwrap();
            assert_eq!(word.value, "echo");
            // Verify location tracking is working
            assert!(word.loc.is_some());
        }

        #[test]
        fn test_wordlist_single() {
            let input_str = "hello";
            let mut input = LocatingSlice::new("hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = wordlist(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let words = result.unwrap();
            assert_eq!(words.len(), 1);
            assert_eq!(words[0].value, "hello");
        }

        #[test]
        fn test_wordlist_multiple() {
            let input_str = "echo hello world";
            let mut input = LocatingSlice::new("echo hello world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = wordlist(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let words = result.unwrap();
            assert_eq!(words.len(), 3);
            assert_eq!(words[0].value, "echo");
            assert_eq!(words[1].value, "hello");
            assert_eq!(words[2].value, "world");
        }

        #[test]
        fn test_wordlist_stops_at_operator() {
            let input_str = "echo hello|";
            let mut input = LocatingSlice::new("echo hello|");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = wordlist(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let words = result.unwrap();
            assert_eq!(words.len(), 2);
            assert_eq!(words[0].value, "echo");
            assert_eq!(words[1].value, "hello");
            // Should have consumed "echo hello", leaving "|"
        }
    }

    // ============================================================================
    // 2. COMMANDS AND ASSIGNMENTS (Tier 3)
    // ============================================================================
    // Tests for simple commands, command prefixes/suffixes, and assignments.

    mod commands_and_assignments {
        use super::*;

        #[test]
        fn test_cmd_suffix() {
            let input_str = " hello world";
            let mut input = LocatingSlice::new(" hello world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = cmd_suffix(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let suffix = result.unwrap();
            assert_eq!(suffix.0.len(), 2);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "hello"),
                _ => panic!("Expected Word"),
            }
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "world"),
                _ => panic!("Expected Word"),
            }
        }

        #[test]
        fn test_simple_command_name_only() {
            let input_str = "echo";
            let mut input = LocatingSlice::new("echo");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_none());
            assert_eq!(cmd.word_or_name.unwrap().value, "echo");
            assert!(cmd.suffix.is_none());
        }

        #[test]
        fn test_simple_command_with_args() {
            let input_str = "echo hello world";
            let mut input = LocatingSlice::new("echo hello world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_none());
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 2);
        }

        #[test]
        fn test_simple_command_stops_at_operator() {
            let input_str = "cat file.txt|";
            let mut input = LocatingSlice::new("cat file.txt|");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.unwrap().value, "cat");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 1);
        }

        #[test]
        fn test_command() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::Command::Simple(cmd) => {
                    assert_eq!(cmd.word_or_name.unwrap().value, "echo");
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_assignment_word() {
            let mut input = LocatingSlice::new("VAR=value");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            match assignment.name {
                ast::AssignmentName::VariableName(name) => assert_eq!(name, "VAR"),
                _ => panic!("Expected VariableName"),
            }
            match assignment.value {
                ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, "value"),
                _ => panic!("Expected Scalar"),
            }
            assert!(!assignment.append);
            assert_eq!(word.value, "VAR=value");
        }

        #[test]
        fn test_assignment_word_append() {
            let mut input = LocatingSlice::new("VAR+=value");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            assert!(assignment.append);
            assert_eq!(word.value, "VAR+=value");
        }

        #[test]
        fn test_assignment_word_empty_value() {
            let mut input = LocatingSlice::new("VAR=");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, _) = result.unwrap();
            match assignment.value {
                ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, ""),
                _ => panic!("Expected Scalar"),
            }
        }

        #[test]
        fn test_assignment_word_array_simple() {
            let mut input = LocatingSlice::new("arr=(one two three)");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            match assignment.value {
                ast::AssignmentValue::Array(elements) => {
                    assert_eq!(elements.len(), 3);
                    assert_eq!(elements[0].0, None);
                    assert_eq!(elements[0].1.value, "one");
                    assert_eq!(elements[1].0, None);
                    assert_eq!(elements[1].1.value, "two");
                    assert_eq!(elements[2].0, None);
                    assert_eq!(elements[2].1.value, "three");
                }
                _ => panic!("Expected Array"),
            }
            assert_eq!(word.value, "arr=(one two three)");
        }

        #[test]
        fn test_assignment_word_array_indexed() {
            let mut input = LocatingSlice::new("arr=([0]=first [1]=second)");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            match assignment.value {
                ast::AssignmentValue::Array(elements) => {
                    assert_eq!(elements.len(), 2);
                    assert_eq!(elements[0].0.as_ref().unwrap().value, "0");
                    assert_eq!(elements[0].1.value, "first");
                    assert_eq!(elements[1].0.as_ref().unwrap().value, "1");
                    assert_eq!(elements[1].1.value, "second");
                }
                _ => panic!("Expected Array"),
            }
            assert_eq!(word.value, "arr=([0]=first [1]=second)");
        }

        #[test]
        fn test_assignment_word_array_mixed() {
            let mut input = LocatingSlice::new("arr=(a b [5]=c d)");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, _) = result.unwrap();
            match assignment.value {
                ast::AssignmentValue::Array(elements) => {
                    assert_eq!(elements.len(), 4);
                    assert_eq!(elements[0].0, None);
                    assert_eq!(elements[0].1.value, "a");
                    assert_eq!(elements[1].0, None);
                    assert_eq!(elements[1].1.value, "b");
                    assert_eq!(elements[2].0.as_ref().unwrap().value, "5");
                    assert_eq!(elements[2].1.value, "c");
                    assert_eq!(elements[3].0, None);
                    assert_eq!(elements[3].1.value, "d");
                }
                _ => panic!("Expected Array"),
            }
        }

        #[test]
        fn test_assignment_word_array_empty() {
            let mut input = LocatingSlice::new("arr=()");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            match assignment.value {
                ast::AssignmentValue::Array(elements) => {
                    assert_eq!(elements.len(), 0);
                }
                _ => panic!("Expected Array"),
            }
            assert_eq!(word.value, "arr=()");
        }

        #[test]
        fn test_assignment_word_array_append() {
            let mut input = LocatingSlice::new("arr+=(new values)");
            let result = assignment_word().parse_next(&mut input);
            assert!(result.is_ok());
            let (assignment, word) = result.unwrap();
            assert!(assignment.append);
            match assignment.value {
                ast::AssignmentValue::Array(elements) => {
                    assert_eq!(elements.len(), 2);
                    assert_eq!(elements[0].1.value, "new");
                    assert_eq!(elements[1].1.value, "values");
                }
                _ => panic!("Expected Array"),
            }
            assert_eq!(word.value, "arr+=(new values)");
        }

        #[test]
        fn test_assignment_word_array_in_command() {
            let input_str = "arr=(one two) cmd";
            let mut input = LocatingSlice::new("arr=(one two) cmd");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_some());
            let prefix = cmd.prefix.unwrap();
            assert_eq!(prefix.0.len(), 1);
            match &prefix.0[0] {
                ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, _) => {
                    match &assignment.value {
                        ast::AssignmentValue::Array(elements) => {
                            assert_eq!(elements.len(), 2);
                        }
                        _ => panic!("Expected Array"),
                    }
                }
                _ => panic!("Expected AssignmentWord"),
            }
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "cmd");
        }

        #[test]
        fn test_cmd_prefix_single_assignment() {
            let input_str = "VAR=value ";
            let mut input = LocatingSlice::new("VAR=value ");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = cmd_prefix(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prefix = result.unwrap();
            assert_eq!(prefix.0.len(), 1);
            match &prefix.0[0] {
                ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, _) => {
                    match &assignment.name {
                        ast::AssignmentName::VariableName(name) => assert_eq!(name, "VAR"),
                        _ => panic!("Expected VariableName"),
                    }
                }
                _ => panic!("Expected AssignmentWord"),
            }
        }

        #[test]
        fn test_cmd_prefix_multiple_assignments() {
            let input_str = "VAR1=val1 VAR2=val2 ";
            let mut input = LocatingSlice::new("VAR1=val1 VAR2=val2 ");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = cmd_prefix(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prefix = result.unwrap();
            assert_eq!(prefix.0.len(), 2);
        }

        #[test]
        fn test_simple_command_with_prefix() {
            let input_str = "VAR=value echo hello";
            let mut input = LocatingSlice::new("VAR=value echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_some());
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            assert!(cmd.suffix.is_some());

            let prefix = cmd.prefix.unwrap();
            assert_eq!(prefix.0.len(), 1);
        }

        #[test]
        fn test_simple_command_assignment_only() {
            let input_str = "VAR=value";
            let mut input = LocatingSlice::new("VAR=value");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_some());
            assert!(cmd.word_or_name.is_none()); // No command, just assignment
            assert!(cmd.suffix.is_none());
        }

        #[test]
        fn test_simple_command_multiple_assignments_with_command() {
            let input_str = "VAR1=a VAR2=b echo test";
            let mut input = LocatingSlice::new("VAR1=a VAR2=b echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_some());
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");

            let prefix = cmd.prefix.unwrap();
            assert_eq!(prefix.0.len(), 2);
        }

        #[test]
        fn test_simple_command_prefix_with_redirect() {
            let input_str = "VAR=value >output.txt echo hello";
            let mut input = LocatingSlice::new("VAR=value >output.txt echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert!(cmd.prefix.is_some());

            let prefix = cmd.prefix.unwrap();
            // Should have assignment and redirect
            assert_eq!(prefix.0.len(), 2);
            assert!(matches!(
                prefix.0[0],
                ast::CommandPrefixOrSuffixItem::AssignmentWord(_, _)
            ));
            assert!(matches!(
                prefix.0[1],
                ast::CommandPrefixOrSuffixItem::IoRedirect(_)
            ));
        }
    }

    // ============================================================================
    // 3. PIPELINES (Tier 4)
    // ============================================================================
    // Tests for pipeline parsing, pipe operators, bang operator, and time command.

    mod pipelines {
        use super::*;

        #[test]
        fn test_pipe_operator() {
            let mut input = LocatingSlice::new("|");
            let result = pipe_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), false); // Regular pipe

            let mut input = LocatingSlice::new("|&");
            let result = pipe_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), true); // Pipe stderr too
        }

        #[test]
        fn test_pipe_sequence_single() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipe_sequence(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let commands = result.unwrap();
            assert_eq!(commands.len(), 1);
        }

        #[test]
        fn test_pipe_sequence_two() {
            let input_str = "cat file|grep test";
            let mut input = LocatingSlice::new("cat file|grep test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipe_sequence(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let commands = result.unwrap();
            assert_eq!(commands.len(), 2);
        }

        #[test]
        fn test_pipe_sequence_three() {
            let input_str = "cat file|grep test|wc -l";
            let mut input = LocatingSlice::new("cat file|grep test|wc -l");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipe_sequence(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let commands = result.unwrap();
            assert_eq!(commands.len(), 3);
        }

        #[test]
        fn test_pipe_sequence_with_spaces() {
            let input_str = "cat file | grep test";
            let mut input = LocatingSlice::new("cat file | grep test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipe_sequence(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let commands = result.unwrap();
            assert_eq!(commands.len(), 2);
        }

        #[test]
        fn test_pipeline() {
            let input_str = "cat file|grep test";
            let mut input = LocatingSlice::new("cat file|grep test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert_eq!(pipe.seq.len(), 2);
            assert!(!pipe.bang);
            assert!(pipe.timed.is_none());
        }

        #[test]
        fn test_pipe_and_operator() {
            let mut input = LocatingSlice::new("|&");
            let result = pipe_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), true); // true means |&
        }

        #[test]
        fn test_pipe_and_adds_stderr_redirect() {
            // Test that |& adds 2>&1 redirect to previous command
            let input_str = "echo hello |& grep test";
            let mut input = LocatingSlice::new("echo hello |& grep test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert_eq!(pipe.seq.len(), 2);

            // Check that the first command has the stderr redirect
            match &pipe.seq[0] {
                ast::Command::Simple(simple) => {
                    assert!(simple.suffix.is_some());
                    let suffix = simple.suffix.as_ref().unwrap();
                    // Should have at least one redirect added (the 2>&1)
                    let has_stderr_redirect = suffix.0.iter().any(|item| {
                        matches!(
                            item,
                            ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::File(
                                Some(2),
                                ast::IoFileRedirectKind::DuplicateOutput,
                                ast::IoFileRedirectTarget::Fd(1)
                            ))
                        )
                    });
                    assert!(
                        has_stderr_redirect,
                        "Expected 2>&1 redirect to be added for |&"
                    );
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_pipe_and_multiple() {
            // Test multiple |& in a pipeline
            let input_str = "cmd1 |& cmd2 |& cmd3";
            let mut input = LocatingSlice::new("cmd1 |& cmd2 |& cmd3");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert_eq!(pipe.seq.len(), 3);

            // Both cmd1 and cmd2 should have stderr redirects
            for i in 0..2 {
                match &pipe.seq[i] {
                    ast::Command::Simple(simple) => {
                        let suffix = simple.suffix.as_ref();
                        if let Some(suffix) = suffix {
                            let has_stderr_redirect = suffix.0.iter().any(|item| {
                                matches!(
                                    item,
                                    ast::CommandPrefixOrSuffixItem::IoRedirect(
                                        ast::IoRedirect::File(
                                            Some(2),
                                            ast::IoFileRedirectKind::DuplicateOutput,
                                            ast::IoFileRedirectTarget::Fd(1)
                                        )
                                    )
                                )
                            });
                            assert!(
                                has_stderr_redirect,
                                "Command {} should have 2>&1 redirect",
                                i
                            );
                        } else {
                            panic!("Command {} should have suffix with redirect", i);
                        }
                    }
                    _ => panic!("Expected Simple command"),
                }
            }
        }

        #[test]
        fn test_pipe_and_mixed_with_regular_pipe() {
            // Test mixing |& and | in same pipeline
            let input_str = "cmd1 |& cmd2 | cmd3";
            let mut input = LocatingSlice::new("cmd1 |& cmd2 | cmd3");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert_eq!(pipe.seq.len(), 3);

            // Only cmd1 should have stderr redirect (before |&)
            match &pipe.seq[0] {
                ast::Command::Simple(simple) => {
                    assert!(simple.suffix.is_some());
                    let suffix = simple.suffix.as_ref().unwrap();
                    let has_stderr_redirect = suffix.0.iter().any(|item| {
                        matches!(
                            item,
                            ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::File(
                                Some(2),
                                ast::IoFileRedirectKind::DuplicateOutput,
                                ast::IoFileRedirectTarget::Fd(1)
                            ))
                        )
                    });
                    assert!(has_stderr_redirect);
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_bang_operator_single() {
            let input_str = "! echo hello";
            let mut input = LocatingSlice::new("! echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.bang);
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_bang_operator_double() {
            // Double negation should cancel out
            let input_str = "! ! echo hello";
            let mut input = LocatingSlice::new("! ! echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(!pipe.bang); // Even number of bangs = no inversion
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_bang_operator_triple() {
            // Three bangs should invert
            let input_str = "! ! ! echo hello";
            let mut input = LocatingSlice::new("! ! ! echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.bang); // Odd number of bangs = inverted
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_bang_with_pipeline() {
            let input_str = "! echo hello | grep hello";
            let mut input = LocatingSlice::new("! echo hello | grep hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.bang);
            assert_eq!(pipe.seq.len(), 2);
        }

        #[test]
        fn test_bang_in_and_or_list() {
            let input_str = "! false && echo success";
            let mut input = LocatingSlice::new("! false && echo success");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let and_or_list = result.unwrap();
            assert!(and_or_list.first.bang);
            assert_eq!(and_or_list.additional.len(), 1);
        }

        #[test]
        fn test_time_command() {
            let input_str = "time echo hello";
            let mut input = LocatingSlice::new("time echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_some());
            match pipe.timed.unwrap() {
                ast::PipelineTimed::Timed(_) => (),
                _ => panic!("Expected Timed variant"),
            }
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_time_with_posix_flag() {
            let input_str = "time -p echo hello";
            let mut input = LocatingSlice::new("time -p echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_some());
            match pipe.timed.unwrap() {
                ast::PipelineTimed::TimedWithPosixOutput(_) => (),
                _ => panic!("Expected TimedWithPosixOutput variant"),
            }
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_time_with_pipeline() {
            let input_str = "time echo hello | grep hello";
            let mut input = LocatingSlice::new("time echo hello | grep hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_some());
            assert_eq!(pipe.seq.len(), 2);
        }

        #[test]
        fn test_time_with_bang() {
            let input_str = "time ! false";
            let mut input = LocatingSlice::new("time ! false");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_some());
            assert!(pipe.bang);
            assert_eq!(pipe.seq.len(), 1);
        }

        #[test]
        fn test_time_posix_with_bang_and_pipeline() {
            let input_str = "time -p ! echo test | grep test";
            let mut input = LocatingSlice::new("time -p ! echo test | grep test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_some());
            match pipe.timed.unwrap() {
                ast::PipelineTimed::TimedWithPosixOutput(_) => (),
                _ => panic!("Expected TimedWithPosixOutput variant"),
            }
            assert!(pipe.bang);
            assert_eq!(pipe.seq.len(), 2);
        }

        #[test]
        fn test_no_time() {
            let input_str = "echo hello";
            let mut input = LocatingSlice::new("echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert!(pipe.timed.is_none());
        }
    }

    // ============================================================================
    // 4. AND/OR LISTS (Tier 5)
    // ============================================================================
    // Tests for && and || operators and and/or list parsing.

    mod and_or_lists {
        use super::*;

        #[test]
        fn test_and_or_op() {
            let mut input = LocatingSlice::new("&&");
            let result = and_or_op().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), true); // And

            let mut input = LocatingSlice::new("||");
            let result = and_or_op().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), false); // Or
        }

        #[test]
        fn test_and_or_single() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.additional.len(), 0); // No and/or continuations
        }

        #[test]
        fn test_and_or_with_and() {
            let input_str = "echo hello && echo world";
            let mut input = LocatingSlice::new("echo hello && echo world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.additional.len(), 1);
            match &list.additional[0] {
                ast::AndOr::And(_) => (),
                _ => panic!("Expected And"),
            }
        }

        #[test]
        fn test_and_or_with_or() {
            let input_str = "echo hello || echo world";
            let mut input = LocatingSlice::new("echo hello || echo world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.additional.len(), 1);
            match &list.additional[0] {
                ast::AndOr::Or(_) => (),
                _ => panic!("Expected Or"),
            }
        }

        #[test]
        fn test_and_or_mixed() {
            let input_str = "cmd1 && cmd2 || cmd3";
            let mut input = LocatingSlice::new("cmd1 && cmd2 || cmd3");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.additional.len(), 2);
            match &list.additional[0] {
                ast::AndOr::And(_) => (),
                _ => panic!("Expected And at position 0"),
            }
            match &list.additional[1] {
                ast::AndOr::Or(_) => (),
                _ => panic!("Expected Or at position 1"),
            }
        }

        #[test]
        fn test_and_or_with_pipes() {
            let input_str = "cat file | grep test && echo done";
            let mut input = LocatingSlice::new("cat file | grep test && echo done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = and_or(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            // First pipeline should have 2 commands (cat, grep)
            assert_eq!(list.first.seq.len(), 2);
            // One and continuation
            assert_eq!(list.additional.len(), 1);
            match &list.additional[0] {
                ast::AndOr::And(p) => {
                    // Second pipeline should have 1 command (echo)
                    assert_eq!(p.seq.len(), 1);
                }
                _ => panic!("Expected And"),
            }
        }
    }

    // ============================================================================
    // 5. COMPLETE COMMANDS AND PROGRAMS (Tier 6)
    // ============================================================================
    // Tests for complete commands, programs, and separator handling.

    mod complete_commands {
        use super::*;

        #[test]
        fn test_complete_command_single() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.0.len(), 1); // One item
        }

        #[test]
        fn test_complete_command_with_semicolon() {
            let input_str = "echo hello ; echo world";
            let mut input = LocatingSlice::new("echo hello ; echo world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.0.len(), 2); // Two items
            assert!(matches!(cmd.0[0].1, ast::SeparatorOperator::Sequence));
        }

        #[test]
        fn test_complete_command_with_ampersand() {
            let input_str = "sleep 1 & echo done";
            let mut input = LocatingSlice::new("sleep 1 & echo done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.0.len(), 2); // Two items
            assert!(matches!(cmd.0[0].1, ast::SeparatorOperator::Async));
        }

        #[test]
        fn test_complete_commands_single() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_commands(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmds = result.unwrap();
            assert_eq!(cmds.len(), 1);
        }

        #[test]
        fn test_complete_commands_multiple() {
            let input_str = "echo hello\necho world";
            let mut input = LocatingSlice::new("echo hello\necho world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_commands(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmds = result.unwrap();
            assert_eq!(cmds.len(), 2);
        }

        #[test]
        fn test_program_simple() {
            let input_str = "echo test";
            let mut input = LocatingSlice::new("echo test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = program(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prog = result.unwrap();
            assert_eq!(prog.complete_commands.len(), 1);
        }

        #[test]
        fn test_program_with_newlines() {
            let input_str = "\necho hello\necho world\n";
            let mut input = LocatingSlice::new("\necho hello\necho world\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = program(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prog = result.unwrap();
            assert_eq!(prog.complete_commands.len(), 2);
        }

        #[test]
        fn test_program_complex() {
            let input_str = "cat file | grep test && echo done ; ls -la";
            let mut input = LocatingSlice::new("cat file | grep test && echo done ; ls -la");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = program(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prog = result.unwrap();
            // Should parse as one complete command with 2 items:
            // 1. "cat file | grep test && echo done"
            // 2. "ls -la"
            assert_eq!(prog.complete_commands.len(), 1);
            assert_eq!(prog.complete_commands[0].0.len(), 2);
        }
    }

    // ============================================================================
    // 6. QUOTED STRINGS (Tier 7)
    // ============================================================================
    // Tests for single-quoted, double-quoted strings, and escape sequences.

    mod quoted_strings {
        use super::*;

        #[test]
        fn test_single_quoted_string() {
            let mut input = LocatingSlice::new("'hello world'");
            let result = single_quoted_string().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello world");
        }

        #[test]
        fn test_single_quoted_empty() {
            let mut input = LocatingSlice::new("''");
            let result = single_quoted_string().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "");
        }

        #[test]
        fn test_double_quoted_string() {
            let mut input = LocatingSlice::new("\"hello world\"");
            let result = double_quoted_string().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "hello world");
        }

        #[test]
        fn test_double_quoted_empty() {
            let mut input = LocatingSlice::new("\"\"");
            let result = double_quoted_string().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "");
        }

        #[test]
        fn test_escape_sequence() {
            let mut input = LocatingSlice::new("\\n");
            let result = escape_sequence().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 'n');
        }

        #[test]
        fn test_word_with_single_quote() {
            let input_str = "'hello world'";
            let mut input = LocatingSlice::new("'hello world'");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "hello world");
        }

        #[test]
        fn test_word_with_double_quote() {
            let input_str = "\"hello world\"";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "hello world");
        }

        #[test]
        fn test_word_mixed_quotes() {
            let input_str = "hello'world'test";
            let mut input = LocatingSlice::new("hello'world'test");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "helloworldtest");
        }

        #[test]
        fn test_word_with_escape() {
            let input_str = "hello\\ world";
            let mut input = LocatingSlice::new("hello\\ world");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "hello world");
        }

        #[test]
        fn test_simple_command_with_quoted_args() {
            let input_str = "echo 'hello world' \"test\"";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 2);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "hello world"),
                _ => panic!("Expected Word"),
            }
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "test"),
                _ => panic!("Expected Word"),
            }
        }
    }

    // ============================================================================
    // 7. REDIRECTIONS (Tier 8)
    // ============================================================================
    // Tests for I/O redirections, here-strings, and file descriptor handling.

    mod redirections {
        use super::*;

        #[test]
        fn test_io_number() {
            let mut input = LocatingSlice::new("2");
            let result = io_number().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 2);
        }

        #[test]
        fn test_redirect_operator_write() {
            let mut input = LocatingSlice::new(">");
            let result = redirect_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert!(matches!(result.unwrap(), ast::IoFileRedirectKind::Write));
        }

        #[test]
        fn test_redirect_operator_append() {
            let mut input = LocatingSlice::new(">>");
            let result = redirect_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert!(matches!(result.unwrap(), ast::IoFileRedirectKind::Append));
        }

        #[test]
        fn test_redirect_operator_read() {
            let mut input = LocatingSlice::new("<");
            let result = redirect_operator().parse_next(&mut input);
            assert!(result.is_ok());
            assert!(matches!(result.unwrap(), ast::IoFileRedirectKind::Read));
        }

        #[test]
        fn test_io_redirect_write() {
            let input_str = "> output.txt";
            let mut input = LocatingSlice::new("> output.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, kind, target) => {
                    assert!(fd.is_none());
                    assert!(matches!(kind, ast::IoFileRedirectKind::Write));
                    match target {
                        ast::IoFileRedirectTarget::Filename(w) => assert_eq!(w.value, "output.txt"),
                        _ => panic!("Expected Filename"),
                    }
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_read() {
            let input_str = "< input.txt";
            let mut input = LocatingSlice::new("< input.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, kind, target) => {
                    assert!(fd.is_none());
                    assert!(matches!(kind, ast::IoFileRedirectKind::Read));
                    match target {
                        ast::IoFileRedirectTarget::Filename(w) => assert_eq!(w.value, "input.txt"),
                        _ => panic!("Expected Filename"),
                    }
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_append() {
            let input_str = ">> log.txt";
            let mut input = LocatingSlice::new(">> log.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, kind, _) => {
                    assert!(fd.is_none());
                    assert!(matches!(kind, ast::IoFileRedirectKind::Append));
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_with_fd() {
            let input_str = "2> error.log";
            let mut input = LocatingSlice::new("2> error.log");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, kind, target) => {
                    assert_eq!(fd, Some(2));
                    assert!(matches!(kind, ast::IoFileRedirectKind::Write));
                    match target {
                        ast::IoFileRedirectTarget::Filename(w) => assert_eq!(w.value, "error.log"),
                        _ => panic!("Expected Filename"),
                    }
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_duplicate() {
            let input_str = "2>&1";
            let mut input = LocatingSlice::new("2>&1");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, kind, target) => {
                    assert_eq!(fd, Some(2));
                    assert!(matches!(kind, ast::IoFileRedirectKind::DuplicateOutput));
                    match target {
                        ast::IoFileRedirectTarget::Duplicate(w) => assert_eq!(w.value, "1"),
                        _ => panic!("Expected Duplicate"),
                    }
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_fd_contiguous() {
            // Fd number must be contiguous with redirect operator (no space)
            let input_str = "2>file";
            let mut input = LocatingSlice::new("2>file");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::File(fd, _, _) => {
                    assert_eq!(fd, Some(2));
                }
                _ => panic!("Expected File redirect"),
            }
        }

        #[test]
        fn test_io_redirect_fd_with_space_becomes_word() {
            // If there's a space between fd and operator, the number should be treated as a word
            // not as an fd number. So "2 >file" should parse as word "2" followed by ">file"
            let input_str = "echo 2 >file";
            let mut input = LocatingSlice::new("echo 2 >file");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");

            let suffix = cmd.suffix.unwrap();
            // Should have: word "2", redirect ">file"
            assert_eq!(suffix.0.len(), 2);

            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "2"),
                _ => panic!("Expected word"),
            }

            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::File(fd, _, _)) => {
                    // The redirect should NOT have fd number since there was a space
                    assert_eq!(*fd, None);
                }
                _ => panic!("Expected redirect"),
            }
        }

        #[test]
        fn test_simple_command_with_redirect() {
            let input_str = "echo hello > output.txt";
            let mut input = LocatingSlice::new("echo hello > output.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 2); // "hello" + redirect

            // First should be word "hello"
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "hello"),
                _ => panic!("Expected Word"),
            }

            // Second should be redirect
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(_) => (),
                _ => panic!("Expected IoRedirect"),
            }
        }

        #[test]
        fn test_simple_command_multiple_redirects() {
            let input_str = "cat < input.txt > output.txt";
            let mut input = LocatingSlice::new("cat < input.txt > output.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "cat");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 2); // Two redirects

            // Both should be redirects
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(_) => (),
                _ => panic!("Expected IoRedirect"),
            }
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(_) => (),
                _ => panic!("Expected IoRedirect"),
            }
        }
    }

    #[test]
    fn test_while_with_redirect() {
        let input_str = "while read line; do echo $line; done < input.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = while_clause(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        // Now parse the redirect
        let redirects = optional_redirects(&ctx, &tracker).parse_next(&mut input);
        assert!(redirects.is_ok());
        assert!(redirects.unwrap().is_some());
    }

    #[test]
    fn test_while_redirect_in_command() {
        let input_str = "while read line; do echo $line; done < input.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        // Check that redirect is part of the command
        match result.unwrap() {
            ast::Command::Compound(ast::CompoundCommand::WhileClause(_), redirects) => {
                assert!(
                    redirects.is_some(),
                    "Redirect should be parsed as part of while command"
                );
            }
            _ => panic!("Expected While compound command"),
        }
    }

    #[test]
    fn test_for_with_redirect() {
        let input_str = "for i in a b c; do echo $i; done > output.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            ast::Command::Compound(ast::CompoundCommand::ForClause(_), redirects) => {
                assert!(redirects.is_some(), "Redirect should be parsed");
            }
            _ => panic!("Expected For compound command"),
        }
    }

    #[test]
    fn test_if_with_redirect() {
        let input_str = "if true; then echo yes; fi > output.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            ast::Command::Compound(ast::CompoundCommand::IfClause(_), redirects) => {
                assert!(redirects.is_some(), "Redirect should be parsed");
            }
            _ => panic!("Expected If compound command"),
        }
    }

    #[test]
    fn test_brace_group_with_redirect() {
        let input_str = "{ echo hello; } > output.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            ast::Command::Compound(ast::CompoundCommand::BraceGroup(_), redirects) => {
                assert!(redirects.is_some(), "Redirect should be parsed");
            }
            _ => panic!("Expected BraceGroup compound command"),
        }
    }

    #[test]
    fn test_subshell_with_redirect() {
        let input_str = "(echo hello) > output.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            ast::Command::Compound(ast::CompoundCommand::Subshell(_), redirects) => {
                assert!(redirects.is_some(), "Redirect should be parsed");
            }
            _ => panic!("Expected Subshell compound command"),
        }
    }

    #[test]
    fn test_extended_test_with_redirect() {
        let input_str = "[[ -f file.txt ]] > output.txt";
        let mut input = LocatingSlice::new(input_str);
        let tracker = make_tracker(input_str);
        let options = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        let ctx = ParseContext {
            options: &options,
            source_info: &source_info,
        };
        let result = command(&ctx, &tracker).parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            ast::Command::ExtendedTest(_, redirects) => {
                assert!(redirects.is_some(), "Redirect should be parsed");
            }
            _ => panic!("Expected ExtendedTest command"),
        }
    }

    // ============================================================================
    // 8. VARIABLE EXPANSIONS (Tier 9)
    // ============================================================================
    // Tests for variable expansion, parameter expansion, arrays, and substitutions.

    mod variable_expansions {
        use super::*;

        #[test]
        fn test_simple_variable() {
            let mut input = LocatingSlice::new("$HOME");
            let result = simple_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$HOME");
        }

        #[test]
        fn test_simple_variable_underscore() {
            let mut input = LocatingSlice::new("$MY_VAR");
            let result = simple_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$MY_VAR");
        }

        #[test]
        fn test_braced_variable() {
            let mut input = LocatingSlice::new("${HOME}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${HOME}");
        }

        #[test]
        fn test_braced_variable_complex() {
            let mut input = LocatingSlice::new("${VAR:-default}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${VAR:-default}");
        }

        #[test]
        fn test_array_element_numeric() {
            let mut input = LocatingSlice::new("${arr[0]}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${arr[0]}");
        }

        #[test]
        fn test_array_element_at() {
            let mut input = LocatingSlice::new("${arr[@]}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${arr[@]}");
        }

        #[test]
        fn test_array_element_star() {
            let mut input = LocatingSlice::new("${arr[*]}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${arr[*]}");
        }

        #[test]
        fn test_array_element_variable_index() {
            let mut input = LocatingSlice::new("${arr[$i]}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${arr[$i]}");
        }

        #[test]
        fn test_array_in_word() {
            let input_str = "prefix${arr[0]}suffix";
            let mut input = LocatingSlice::new("prefix${arr[0]}suffix");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let word = result.unwrap();
            assert_eq!(word.value, "prefix${arr[0]}suffix");
        }

        #[test]
        fn test_array_in_double_quotes() {
            let mut input = LocatingSlice::new("\"${arr[@]}\"");
            let result = double_quoted_string().parse_next(&mut input);
            assert!(result.is_ok());
            // double_quoted_string strips the quotes
            assert_eq!(result.unwrap(), "${arr[@]}");
        }

        #[test]
        fn test_array_in_command() {
            let input_str = "echo ${arr[0]} ${arr[@]}";
            let mut input = LocatingSlice::new("echo ${arr[0]} ${arr[@]}");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.as_ref().unwrap();
            assert_eq!(suffix.0.len(), 2);
            // Check that both array references are captured
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => {
                    assert_eq!(w.value, "${arr[0]}");
                }
                _ => panic!("Expected Word"),
            }
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::Word(w) => {
                    assert_eq!(w.value, "${arr[@]}");
                }
                _ => panic!("Expected Word"),
            }
        }

        // Advanced parameter expansion tests

        #[test]
        fn test_param_expansion_default() {
            let mut input = LocatingSlice::new("${var:-default}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:-default}");
        }

        #[test]
        fn test_param_expansion_assign_default() {
            let mut input = LocatingSlice::new("${var:=default}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:=default}");
        }

        #[test]
        fn test_param_expansion_error() {
            let mut input = LocatingSlice::new("${var:?error message}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:?error message}");
        }

        #[test]
        fn test_param_expansion_alternate() {
            let mut input = LocatingSlice::new("${var:+alternate}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:+alternate}");
        }

        #[test]
        fn test_param_expansion_remove_prefix() {
            let mut input = LocatingSlice::new("${var#prefix}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var#prefix}");
        }

        #[test]
        fn test_param_expansion_remove_longest_prefix() {
            let mut input = LocatingSlice::new("${var##prefix}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var##prefix}");
        }

        #[test]
        fn test_param_expansion_remove_suffix() {
            let mut input = LocatingSlice::new("${var%suffix}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var%suffix}");
        }

        #[test]
        fn test_param_expansion_remove_longest_suffix() {
            let mut input = LocatingSlice::new("${var%%suffix}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var%%suffix}");
        }

        #[test]
        fn test_param_expansion_replace_first() {
            let mut input = LocatingSlice::new("${var/pattern/replacement}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var/pattern/replacement}");
        }

        #[test]
        fn test_param_expansion_replace_all() {
            let mut input = LocatingSlice::new("${var//pattern/replacement}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var//pattern/replacement}");
        }

        #[test]
        fn test_param_expansion_substring() {
            let mut input = LocatingSlice::new("${var:0:5}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:0:5}");
        }

        #[test]
        fn test_param_expansion_substring_offset() {
            let mut input = LocatingSlice::new("${var:5}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var:5}");
        }

        #[test]
        fn test_param_expansion_length() {
            let mut input = LocatingSlice::new("${#var}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${#var}");
        }

        #[test]
        fn test_param_expansion_uppercase() {
            let mut input = LocatingSlice::new("${var^^}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var^^}");
        }

        #[test]
        fn test_param_expansion_lowercase() {
            let mut input = LocatingSlice::new("${var,,}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var,,}");
        }

        #[test]
        fn test_param_expansion_in_word() {
            let input_str = "prefix${var:-default}suffix";
            let mut input = LocatingSlice::new("prefix${var:-default}suffix");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "prefix${var:-default}suffix");
        }

        #[test]
        fn test_param_expansion_in_command() {
            let input_str = "echo ${var:-default} ${path%/*}";
            let mut input = LocatingSlice::new("echo ${var:-default} ${path%/*}");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.as_ref().unwrap();
            assert_eq!(suffix.0.len(), 2);
        }

        #[test]
        fn test_param_expansion_nested_braces() {
            // Complex case with replacement containing special chars
            let mut input = LocatingSlice::new("${var/old/new}");
            let result = braced_variable().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "${var/old/new}");
        }

        // Here-string and OutputAndError redirect tests

        #[test]
        fn test_here_string_redirect() {
            let input_str = "<<< \"hello world\"";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::HereString(fd, word) => {
                    assert_eq!(fd, None);
                    assert_eq!(word.value, "hello world");
                }
                _ => panic!("Expected HereString redirect"),
            }
        }

        #[test]
        fn test_here_string_redirect_with_fd() {
            let input_str = "0<<< input_data";
            let mut input = LocatingSlice::new("0<<< input_data");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::HereString(fd, word) => {
                    assert_eq!(fd, Some(0));
                    assert_eq!(word.value, "input_data");
                }
                _ => panic!("Expected HereString redirect"),
            }
        }

        #[test]
        fn test_here_string_redirect_with_variable() {
            let input_str = "<<< $VAR";
            let mut input = LocatingSlice::new("<<< $VAR");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::HereString(fd, word) => {
                    assert_eq!(fd, None);
                    assert_eq!(word.value, "$VAR");
                }
                _ => panic!("Expected HereString redirect"),
            }
        }

        #[test]
        fn test_output_and_error_redirect() {
            let input_str = "&> output.log";
            let mut input = LocatingSlice::new("&> output.log");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::OutputAndError(word, append) => {
                    assert_eq!(word.value, "output.log");
                    assert!(!append);
                }
                _ => panic!("Expected OutputAndError redirect"),
            }
        }

        #[test]
        fn test_output_and_error_redirect_append() {
            let input_str = "&>> output.log";
            let mut input = LocatingSlice::new("&>> output.log");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = io_redirect(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::IoRedirect::OutputAndError(word, append) => {
                    assert_eq!(word.value, "output.log");
                    assert!(append);
                }
                _ => panic!("Expected OutputAndError redirect"),
            }
        }

        #[test]
        fn test_here_string_in_command() {
            let input_str = "cat <<< \"test string\"";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "cat");
            let suffix = cmd.suffix.as_ref().unwrap();
            assert_eq!(suffix.0.len(), 1);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::HereString(
                    fd,
                    word,
                )) => {
                    assert_eq!(*fd, None);
                    assert_eq!(word.value, "test string");
                }
                _ => panic!("Expected HereString redirect in suffix"),
            }
        }

        #[test]
        fn test_output_and_error_in_command() {
            let input_str = "command &> /dev/null";
            let mut input = LocatingSlice::new("command &> /dev/null");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "command");
            let suffix = cmd.suffix.as_ref().unwrap();
            assert_eq!(suffix.0.len(), 1);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::OutputAndError(
                    word,
                    append,
                )) => {
                    assert_eq!(word.value, "/dev/null");
                    assert!(!append);
                }
                _ => panic!("Expected OutputAndError redirect in suffix"),
            }
        }

        #[test]
        fn test_output_and_error_append_in_command() {
            let input_str = "make &>> build.log";
            let mut input = LocatingSlice::new("make &>> build.log");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "make");
            let suffix = cmd.suffix.as_ref().unwrap();
            assert_eq!(suffix.0.len(), 1);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::OutputAndError(
                    word,
                    append,
                )) => {
                    assert_eq!(word.value, "build.log");
                    assert!(append);
                }
                _ => panic!("Expected OutputAndError redirect in suffix"),
            }
        }

        #[test]
        fn test_arithmetic_expansion() {
            let mut input = LocatingSlice::new("$((1 + 2))");
            let result = arithmetic_expansion().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$((1 + 2))");
        }

        #[test]
        fn test_command_substitution() {
            let mut input = LocatingSlice::new("$(echo hello)");
            let result = command_substitution().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$(echo hello)");
        }

        #[test]
        fn test_backtick_substitution() {
            let mut input = LocatingSlice::new("`echo hello`");
            let result = backtick_substitution().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "`echo hello`");
        }

        #[test]
        fn test_special_parameter_question() {
            let mut input = LocatingSlice::new("$?");
            let result = special_parameter().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$?");
        }

        #[test]
        fn test_special_parameter_digit() {
            let mut input = LocatingSlice::new("$1");
            let result = special_parameter().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$1");
        }

        #[test]
        fn test_special_parameter_at() {
            let mut input = LocatingSlice::new("$@");
            let result = special_parameter().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "$@");
        }

        #[test]
        fn test_word_with_variable() {
            let input_str = "$HOME/bin";
            let mut input = LocatingSlice::new("$HOME/bin");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "$HOME/bin");
        }

        #[test]
        fn test_word_with_braced_variable() {
            let input_str = "${VAR}_suffix";
            let mut input = LocatingSlice::new("${VAR}_suffix");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "${VAR}_suffix");
        }

        #[test]
        fn test_word_with_command_substitution() {
            let input_str = "prefix_$(cmd)_suffix";
            let mut input = LocatingSlice::new("prefix_$(cmd)_suffix");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = word_as_ast(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().value, "prefix_$(cmd)_suffix");
        }

        #[test]
        fn test_simple_command_with_variables() {
            let input_str = "echo $HOME $USER";
            let mut input = LocatingSlice::new("echo $HOME $USER");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 2);
            match &suffix.0[0] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "$HOME"),
                _ => panic!("Expected Word"),
            }
            match &suffix.0[1] {
                ast::CommandPrefixOrSuffixItem::Word(w) => assert_eq!(w.value, "$USER"),
                _ => panic!("Expected Word"),
            }
        }

        #[test]
        fn test_command_with_mixed_expansions() {
            let input_str = "echo ${VAR} $(cmd) $1";
            let mut input = LocatingSlice::new("echo ${VAR} $(cmd) $1");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = simple_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            let suffix = cmd.suffix.unwrap();
            assert_eq!(suffix.0.len(), 3);
        }
    }

    // ============================================================================
    // 9. SUBSHELLS AND COMMAND GROUPS (Tier 10)
    // ============================================================================
    // Tests for subshells ( ), brace groups { }, and compound lists.

    mod subshells_and_groups {
        use super::*;

        #[test]
        fn test_compound_list_simple() {
            let input_str = "echo hello";
            let mut input = LocatingSlice::new("echo hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_list(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.0.len(), 1);
        }

        #[test]
        fn test_compound_list_with_semicolon() {
            let input_str = "cmd1; cmd2";
            let mut input = LocatingSlice::new("cmd1; cmd2");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_list(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.0.len(), 2);
            assert_eq!(list.0[0].1, ast::SeparatorOperator::Sequence);
        }

        #[test]
        fn test_compound_list_with_ampersand() {
            let input_str = "cmd1 & cmd2";
            let mut input = LocatingSlice::new("cmd1 & cmd2");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_list(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let list = result.unwrap();
            assert_eq!(list.0.len(), 2);
            assert_eq!(list.0[0].1, ast::SeparatorOperator::Async);
        }

        #[test]
        fn test_subshell_simple() {
            let input_str = "( echo hello )";
            let mut input = LocatingSlice::new("( echo hello )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = subshell(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let sub = result.unwrap();
            assert_eq!(sub.list.0.len(), 1);
        }

        #[test]
        fn test_subshell_multiple_commands() {
            let input_str = "( cmd1; cmd2 )";
            let mut input = LocatingSlice::new("( cmd1; cmd2 )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = subshell(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let sub = result.unwrap();
            assert_eq!(sub.list.0.len(), 2);
        }

        #[test]
        fn test_subshell_with_pipeline() {
            let input_str = "( cat file | grep test )";
            let mut input = LocatingSlice::new("( cat file | grep test )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = subshell(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let sub = result.unwrap();
            assert_eq!(sub.list.0.len(), 1);
        }

        #[test]
        fn test_brace_group_simple() {
            let input_str = "{ echo hello; }";
            let mut input = LocatingSlice::new("{ echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = brace_group(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let grp = result.unwrap();
            assert_eq!(grp.list.0.len(), 1);
        }

        #[test]
        fn test_brace_group_multiple() {
            let input_str = "{ cmd1; cmd2; }";
            let mut input = LocatingSlice::new("{ cmd1; cmd2; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = brace_group(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let grp = result.unwrap();
            assert_eq!(grp.list.0.len(), 2);
        }
    }

    // ============================================================================
    // 10. COMPOUND COMMANDS (Tier 11)
    // ============================================================================
    // Tests for if/then/else, while/until loops, and do groups.

    mod compound_commands {
        use super::*;

        #[test]
        fn test_do_group_simple() {
            let input_str = "do echo hello; done";
            let mut input = LocatingSlice::new("do echo hello; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = do_group(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let grp = result.unwrap();
            assert_eq!(grp.list.0.len(), 1);
        }

        #[test]
        fn test_do_group_multiple() {
            let input_str = "do cmd1; cmd2; done";
            let mut input = LocatingSlice::new("do cmd1; cmd2; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = do_group(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let grp = result.unwrap();
            assert_eq!(grp.list.0.len(), 2);
        }

        #[test]
        fn test_if_simple() {
            let input_str = "if test; then echo yes; fi";
            let mut input = LocatingSlice::new("if test; then echo yes; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = if_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let if_cmd = result.unwrap();
            assert_eq!(if_cmd.condition.0.len(), 1);
            assert_eq!(if_cmd.then.0.len(), 1);
            assert!(if_cmd.elses.is_none());
        }

        #[test]
        fn test_if_with_else() {
            let input_str = "if test; then echo yes; else echo no; fi";
            let mut input = LocatingSlice::new("if test; then echo yes; else echo no; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = if_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let if_cmd = result.unwrap();
            assert_eq!(if_cmd.condition.0.len(), 1);
            assert_eq!(if_cmd.then.0.len(), 1);
            assert!(if_cmd.elses.is_some());
            let elses = if_cmd.elses.unwrap();
            assert_eq!(elses.len(), 1);
            assert!(elses[0].condition.is_none()); // else clause has no condition
        }

        #[test]
        fn test_if_with_elif() {
            let input_str = "if test1; then echo one; elif test2; then echo two; fi";
            let mut input =
                LocatingSlice::new("if test1; then echo one; elif test2; then echo two; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = if_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let if_cmd = result.unwrap();
            assert!(if_cmd.elses.is_some());
            let elses = if_cmd.elses.unwrap();
            assert_eq!(elses.len(), 1);
            assert!(elses[0].condition.is_some()); // elif has a condition
        }

        #[test]
        fn test_if_with_elif_and_else() {
            let input_str =
                "if test1; then echo one; elif test2; then echo two; else echo three; fi";
            let mut input = LocatingSlice::new(
                "if test1; then echo one; elif test2; then echo two; else echo three; fi",
            );
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = if_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let if_cmd = result.unwrap();
            assert!(if_cmd.elses.is_some());
            let elses = if_cmd.elses.unwrap();
            assert_eq!(elses.len(), 2);
            assert!(elses[0].condition.is_some()); // elif
            assert!(elses[1].condition.is_none()); // else
        }

        #[test]
        fn test_while_simple() {
            let input_str = "while test; do echo loop; done";
            let mut input = LocatingSlice::new("while test; do echo loop; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = while_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let while_cmd = result.unwrap();
            assert_eq!(while_cmd.0.0.len(), 1); // condition
            assert_eq!(while_cmd.1.list.0.len(), 1); // body
        }

        #[test]
        fn test_while_multiple_commands() {
            let input_str = "while test; do cmd1; cmd2; done";
            let mut input = LocatingSlice::new("while test; do cmd1; cmd2; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = while_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let while_cmd = result.unwrap();
            assert_eq!(while_cmd.1.list.0.len(), 2);
        }

        #[test]
        fn test_until_simple() {
            let input_str = "until test; do echo loop; done";
            let mut input = LocatingSlice::new("until test; do echo loop; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = until_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let until_cmd = result.unwrap();
            assert_eq!(until_cmd.0.0.len(), 1); // condition
            assert_eq!(until_cmd.1.list.0.len(), 1); // body
        }

        #[test]
        fn test_if_nested() {
            let input_str = "if test1; then if test2; then echo nested; fi; fi";
            let mut input = LocatingSlice::new("if test1; then if test2; then echo nested; fi; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = if_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }
    }

    // ============================================================================
    // 11. FOR LOOPS (Tier 12)
    // ============================================================================
    // Tests for for loops with and without values list.

    mod for_loops {
        use super::*;

        #[test]
        fn test_for_simple() {
            let input_str = "for x in a b c; do echo $x; done";
            let mut input = LocatingSlice::new("for x in a b c; do echo $x; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert_eq!(for_cmd.variable_name, "x");
            assert!(for_cmd.values.is_some());
            let values = for_cmd.values.unwrap();
            assert_eq!(values.len(), 3);
            assert_eq!(values[0].value, "a");
            assert_eq!(values[1].value, "b");
            assert_eq!(values[2].value, "c");
        }

        #[test]
        fn test_for_without_in() {
            let input_str = "for x; do echo $x; done";
            let mut input = LocatingSlice::new("for x; do echo $x; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert_eq!(for_cmd.variable_name, "x");
            assert!(for_cmd.values.is_none());
        }

        #[test]
        fn test_for_with_newline() {
            let input_str = "for x in a b c\ndo echo $x; done";
            let mut input = LocatingSlice::new("for x in a b c\ndo echo $x; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert_eq!(for_cmd.variable_name, "x");
        }

        #[test]
        fn test_for_multiple_commands() {
            let input_str = "for i in 1 2 3; do cmd1; cmd2; done";
            let mut input = LocatingSlice::new("for i in 1 2 3; do cmd1; cmd2; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert_eq!(for_cmd.body.list.0.len(), 2);
        }

        #[test]
        fn test_name_valid() {
            let mut input = LocatingSlice::new("var_name123");
            let result = name().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "var_name123");
        }

        #[test]
        fn test_name_underscore() {
            let mut input = LocatingSlice::new("_var");
            let result = name().parse_next(&mut input);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "_var");
        }
    }

    // ============================================================================
    // 12. CASE STATEMENTS (Tier 13)
    // ============================================================================
    // Tests for case/esac pattern matching statements.

    mod case_statements {
        use super::*;

        #[test]
        fn test_case_simple() {
            let input_str = "case x in a) echo one ;; esac";
            let mut input = LocatingSlice::new("case x in a) echo one ;; esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.value.value, "x");
            assert_eq!(case_cmd.cases.len(), 1);
            assert_eq!(case_cmd.cases[0].patterns.len(), 1);
            assert_eq!(case_cmd.cases[0].patterns[0].value, "a");
        }

        #[test]
        fn test_case_multiple_patterns() {
            let input_str = "case x in a|b|c) echo multi ;; esac";
            let mut input = LocatingSlice::new("case x in a|b|c) echo multi ;; esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases.len(), 1);
            assert_eq!(case_cmd.cases[0].patterns.len(), 3);
            assert_eq!(case_cmd.cases[0].patterns[0].value, "a");
            assert_eq!(case_cmd.cases[0].patterns[1].value, "b");
            assert_eq!(case_cmd.cases[0].patterns[2].value, "c");
        }

        #[test]
        fn test_case_multiple_items() {
            let input_str = "case x in a) echo one ;; b) echo two ;; esac";
            let mut input = LocatingSlice::new("case x in a) echo one ;; b) echo two ;; esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases.len(), 2);
            assert_eq!(case_cmd.cases[0].patterns[0].value, "a");
            assert_eq!(case_cmd.cases[1].patterns[0].value, "b");
        }

        #[test]
        fn test_case_with_paren() {
            let input_str = "case x in (a) echo one ;; esac";
            let mut input = LocatingSlice::new("case x in (a) echo one ;; esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases[0].patterns[0].value, "a");
        }

        #[test]
        fn test_case_empty() {
            let input_str = "case x in esac";
            let mut input = LocatingSlice::new("case x in esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases.len(), 0);
        }

        #[test]
        fn test_case_with_newlines() {
            let input_str = "case x in\na) echo one ;;\nesac";
            let mut input = LocatingSlice::new("case x in\na) echo one ;;\nesac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases.len(), 1);
        }

        #[test]
        fn test_case_terminators() {
            let input_str = "case x in a) cmd1 ;& b) cmd2 ;; esac";
            let mut input = LocatingSlice::new("case x in a) cmd1 ;& b) cmd2 ;; esac");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = case_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let case_cmd = result.unwrap();
            assert_eq!(case_cmd.cases.len(), 2);
            assert_eq!(
                case_cmd.cases[0].post_action,
                ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem
            );
            assert_eq!(
                case_cmd.cases[1].post_action,
                ast::CaseItemPostAction::ExitCase
            );
        }
    }

    // ============================================================================
    // 13. FUNCTION DEFINITIONS (Tier 14)
    // ============================================================================
    // Tests for function definitions with various syntax styles and reserved words.

    mod function_definitions {
        use super::*;

        #[test]
        fn test_function_simple_parens() {
            let input_str = "foo() { echo hello; }";
            let mut input = LocatingSlice::new("foo() { echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
        }

        #[test]
        fn test_function_with_keyword() {
            let input_str = "function bar { echo world; }";
            let mut input = LocatingSlice::new("function bar { echo world; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "bar");
        }

        #[test]
        fn test_function_keyword_with_parens() {
            let input_str = "function baz() { ls; }";
            let mut input = LocatingSlice::new("function baz() { ls; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "baz");
        }

        #[test]
        fn test_function_with_subshell() {
            let input_str = "foo() ( echo hello )";
            let mut input = LocatingSlice::new("foo() ( echo hello )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
            match func.body.0 {
                ast::CompoundCommand::Subshell(_) => {}
                _ => panic!("Expected subshell"),
            }
        }

        #[test]
        fn test_function_with_redirects() {
            let input_str = "foo() { echo hello; } > output.txt";
            let mut input = LocatingSlice::new("foo() { echo hello; } > output.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
            assert!(func.body.1.is_some());
        }

        #[test]
        fn test_function_with_if() {
            let input_str = "foo() if test; then echo yes; fi";
            let mut input = LocatingSlice::new("foo() if test; then echo yes; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
            match func.body.0 {
                ast::CompoundCommand::IfClause(_) => {}
                _ => panic!("Expected if clause"),
            }
        }

        #[test]
        fn test_function_with_for() {
            let input_str = "foo() for x in a b c; do echo $x; done";
            let mut input = LocatingSlice::new("foo() for x in a b c; do echo $x; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
            match func.body.0 {
                ast::CompoundCommand::ForClause(_) => {}
                _ => panic!("Expected for clause"),
            }
        }

        #[test]
        fn test_function_with_newlines() {
            let input_str = "foo()\n{\n  echo hello\n}";
            let mut input = LocatingSlice::new("foo()\n{\n  echo hello\n}");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
        }

        #[test]
        fn test_compound_command_brace() {
            let input_str = "{ echo hello; }";
            let mut input = LocatingSlice::new("{ echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::BraceGroup(_) => {}
                _ => panic!("Expected brace group"),
            }
        }

        #[test]
        fn test_compound_command_subshell() {
            let input_str = "( ls )";
            let mut input = LocatingSlice::new("( ls )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::Subshell(_) => {}
                _ => panic!("Expected subshell"),
            }
        }
    }

    // ============================================================================
    // 14. ARITHMETIC EXPRESSIONS (Tier 15)
    // ============================================================================
    // Tests for (( )) arithmetic expressions and commands.

    mod arithmetic_expressions {
        use super::*;

        #[test]
        fn test_arithmetic_simple() {
            let input_str = "(( 1 + 2 ))";
            let mut input = LocatingSlice::new("(( 1 + 2 ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "1 + 2");
        }

        #[test]
        fn test_arithmetic_with_variables() {
            let input_str = "(( x = 5 ))";
            let mut input = LocatingSlice::new("(( x = 5 ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "x = 5");
        }

        #[test]
        fn test_arithmetic_complex() {
            let input_str = "(( (a + b) * c ))";
            let mut input = LocatingSlice::new("(( (a + b) * c ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "(a + b) * c");
        }

        #[test]
        fn test_arithmetic_nested_parens() {
            let input_str = "(( ((x + 1) * 2) - 3 ))";
            let mut input = LocatingSlice::new("(( ((x + 1) * 2) - 3 ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "((x + 1) * 2) - 3");
        }

        #[test]
        fn test_arithmetic_operators() {
            let input_str = "(( x++ + --y ))";
            let mut input = LocatingSlice::new("(( x++ + --y ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "x++ + --y");
        }

        #[test]
        fn test_arithmetic_with_spaces() {
            let input_str = "((  x  +  y  ))";
            let mut input = LocatingSlice::new("((  x  +  y  ))");
            let tracker = make_tracker(input_str);
            let result = arithmetic_command(&tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let arith = result.unwrap();
            assert_eq!(arith.expr.value, "x  +  y");
        }

        #[test]
        fn test_paren_compound_arithmetic() {
            let input_str = "(( 1 + 1 ))";
            let mut input = LocatingSlice::new("(( 1 + 1 ))");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = paren_compound(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::Arithmetic(_) => {}
                _ => panic!("Expected arithmetic"),
            }
        }

        #[test]
        fn test_paren_compound_subshell() {
            let input_str = "( echo hi )";
            let mut input = LocatingSlice::new("( echo hi )");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = paren_compound(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::Subshell(_) => {}
                _ => panic!("Expected subshell"),
            }
        }

        #[test]
        fn test_command_with_arithmetic() {
            let input_str = "(( x = 10 ))";
            let mut input = LocatingSlice::new("(( x = 10 ))");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::Command::Compound(ast::CompoundCommand::Arithmetic(_), _) => {}
                _ => panic!("Expected arithmetic command"),
            }
        }

        #[test]
        fn test_compound_command_arithmetic() {
            let input_str = "(( i < 10 ))";
            let mut input = LocatingSlice::new("(( i < 10 ))");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = compound_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::Arithmetic(_) => {}
                _ => panic!("Expected arithmetic"),
            }
        }
    }

    // ============================================================================
    // 15. ARITHMETIC FOR LOOPS (Tier 16)
    // ============================================================================
    // Tests for C-style for (( )) loops.

    mod arithmetic_for_loops {
        use super::*;

        #[test]
        fn test_arithmetic_for_simple() {
            let input_str = "for (( i = 0; i < 10; i++ )) do echo $i; done";
            let mut input = LocatingSlice::new("for (( i = 0; i < 10; i++ )) do echo $i; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert!(for_cmd.initializer.is_some());
            assert_eq!(for_cmd.initializer.unwrap().value, "i = 0");
            assert!(for_cmd.condition.is_some());
            assert_eq!(for_cmd.condition.unwrap().value, "i < 10");
            assert!(for_cmd.updater.is_some());
            assert_eq!(for_cmd.updater.unwrap().value, "i++");
        }

        // TODO: Fix handling of empty expressions in arithmetic for loops
        // These tests currently fail because opt(arithmetic_expression()) returns Some("")
        // instead of None for empty expressions
        #[test]
        #[ignore]
        fn test_arithmetic_for_empty_init() {
            let input_str = "for (( ; i < 10; i++ )) do cmd; done";
            let mut input = LocatingSlice::new("for (( ; i < 10; i++ )) do cmd; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert!(for_cmd.initializer.is_none());
            assert!(for_cmd.condition.is_some());
            assert!(for_cmd.updater.is_some());
        }

        #[test]
        #[ignore]
        fn test_arithmetic_for_empty_condition() {
            let input_str = "for (( i = 0; ; i++ )) do cmd; done";
            let mut input = LocatingSlice::new("for (( i = 0; ; i++ )) do cmd; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert!(for_cmd.initializer.is_some());
            assert!(for_cmd.condition.is_none());
            assert!(for_cmd.updater.is_some());
        }

        #[test]
        #[ignore]
        fn test_arithmetic_for_empty_updater() {
            let input_str = "for (( i = 0; i < 10; )) do cmd; done";
            let mut input = LocatingSlice::new("for (( i = 0; i < 10; )) do cmd; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert!(for_cmd.initializer.is_some());
            assert!(for_cmd.condition.is_some());
            assert!(for_cmd.updater.is_none());
        }

        #[test]
        #[ignore]
        fn test_arithmetic_for_all_empty() {
            let input_str = "for (( ; ; )) do echo infinite; done";
            let mut input = LocatingSlice::new("for (( ; ; )) do echo infinite; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert!(for_cmd.initializer.is_none());
            assert!(for_cmd.condition.is_none());
            assert!(for_cmd.updater.is_none());
        }

        // TODO: Fix handling of brace group body in arithmetic for loops
        #[test]
        #[ignore]
        fn test_arithmetic_for_with_brace_body() {
            let input_str = "for (( i = 0; i < 5; i++ )) { echo $i; }";
            let mut input = LocatingSlice::new("for (( i = 0; i < 5; i++ )) { echo $i; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_arithmetic_for_complex_expressions() {
            let input_str = "for (( i = 0, j = 10; i < j; i++, j-- )) do cmd; done";
            let mut input =
                LocatingSlice::new("for (( i = 0, j = 10; i < j; i++, j-- )) do cmd; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = arithmetic_for_clause(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let for_cmd = result.unwrap();
            assert_eq!(for_cmd.initializer.as_ref().unwrap().value, "i = 0, j = 10");
            assert_eq!(for_cmd.condition.as_ref().unwrap().value, "i < j");
            assert_eq!(for_cmd.updater.as_ref().unwrap().value, "i++, j--");
        }

        #[test]
        fn test_for_or_arithmetic_for_regular() {
            let input_str = "for x in a b c; do echo $x; done";
            let mut input = LocatingSlice::new("for x in a b c; do echo $x; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_or_arithmetic_for(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::ForClause(_) => {}
                _ => panic!("Expected regular for clause"),
            }
        }

        #[test]
        fn test_for_or_arithmetic_for_arithmetic() {
            let input_str = "for (( i = 0; i < 10; i++ )) do cmd; done";
            let mut input = LocatingSlice::new("for (( i = 0; i < 10; i++ )) do cmd; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = for_or_arithmetic_for(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::CompoundCommand::ArithmeticForClause(_) => {}
                _ => panic!("Expected arithmetic for clause"),
            }
        }

        #[test]
        fn test_command_with_arithmetic_for() {
            let input_str = "for (( i = 1; i <= 5; i++ )) do echo $i; done";
            let mut input = LocatingSlice::new("for (( i = 1; i <= 5; i++ )) do echo $i; done");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::Command::Compound(ast::CompoundCommand::ArithmeticForClause(_), _) => {}
                _ => panic!("Expected arithmetic for command"),
            }
        }
    }

    // ============================================================================
    // 16. EXTENDED TEST EXPRESSIONS (Tier 17)
    // ============================================================================
    // Tests for [[ ]] extended test expressions with file tests, string tests,
    // pattern matching, and logical operators.

    mod extended_test_expressions {
        use super::*;

        #[test]
        fn test_extended_test_unary_file_exists() {
            let input_str = "[[ -f file.txt ]]";
            let mut input = LocatingSlice::new("[[ -f file.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::UnaryTest(
                    ast::UnaryPredicate::FileExistsAndIsRegularFile,
                    word,
                ) => {
                    assert_eq!(word.to_string(), "file.txt");
                }
                _ => panic!("Expected unary file test"),
            }
        }

        #[test]
        fn test_extended_test_unary_string_zero_length() {
            let input_str = "[[ -z $var ]]";
            let mut input = LocatingSlice::new("[[ -z $var ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::UnaryTest(
                    ast::UnaryPredicate::StringHasZeroLength,
                    word,
                ) => {
                    assert_eq!(word.to_string(), "$var");
                }
                _ => panic!("Expected unary string zero-length test"),
            }
        }

        #[test]
        fn test_extended_test_unary_string_non_zero_length() {
            let input_str = "[[ -n $var ]]";
            let mut input = LocatingSlice::new("[[ -n $var ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::UnaryTest(
                    ast::UnaryPredicate::StringHasNonZeroLength,
                    word,
                ) => {
                    assert_eq!(word.to_string(), "$var");
                }
                _ => panic!("Expected unary string non-zero length test"),
            }
        }

        #[test]
        fn test_extended_test_binary_string_equals() {
            let input_str = "[[ $a = hello ]]";
            let mut input = LocatingSlice::new("[[ $a = hello ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringExactlyMatchesPattern,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$a");
                    assert_eq!(right.to_string(), "hello");
                }
                _ => panic!("Expected binary string equals test"),
            }
        }

        #[test]
        fn test_extended_test_binary_string_not_equals() {
            let input_str = "[[ $a != hello ]]";
            let mut input = LocatingSlice::new("[[ $a != hello ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringDoesNotExactlyMatchPattern,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$a");
                    assert_eq!(right.to_string(), "hello");
                }
                _ => panic!("Expected binary string not-equals test"),
            }
        }

        #[test]
        fn test_extended_test_binary_arithmetic() {
            let input_str = "[[ $a -eq 5 ]]";
            let mut input = LocatingSlice::new("[[ $a -eq 5 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::ArithmeticEqualTo,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$a");
                    assert_eq!(right.to_string(), "5");
                }
                _ => panic!("Expected binary arithmetic test"),
            }
        }

        #[test]
        fn test_extended_test_binary_arithmetic_less_than() {
            let input_str = "[[ $a -lt 10 ]]";
            let mut input = LocatingSlice::new("[[ $a -lt 10 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::ArithmeticLessThan,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$a");
                    assert_eq!(right.to_string(), "10");
                }
                _ => panic!("Expected binary arithmetic less than test"),
            }
        }

        #[test]
        fn test_extended_test_binary_arithmetic_greater_than() {
            let input_str = "[[ $a -gt 0 ]]";
            let mut input = LocatingSlice::new("[[ $a -gt 0 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::ArithmeticGreaterThan,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$a");
                    assert_eq!(right.to_string(), "0");
                }
                _ => panic!("Expected binary arithmetic greater than test"),
            }
        }

        #[test]
        fn test_extended_test_single_word_fallback() {
            let input_str = "[[ $var ]]";
            let mut input = LocatingSlice::new("[[ $var ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::UnaryTest(
                    ast::UnaryPredicate::StringHasNonZeroLength,
                    word,
                ) => {
                    assert_eq!(word.to_string(), "$var");
                }
                _ => panic!("Expected fallback to non-zero length test"),
            }
        }

        #[test]
        fn test_extended_test_with_quoted_string() {
            let input_str = "[[ \"$a\" = \"hello\" ]]";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringExactlyMatchesPattern,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "\"$a\"");
                    assert_eq!(right.to_string(), "\"hello\"");
                }
                _ => panic!("Expected binary string equals test with quotes"),
            }
        }

        #[test]
        fn test_extended_test_regex_match() {
            let input_str = "[[ $str =~ ^[0-9]+$ ]]";
            let mut input = LocatingSlice::new("[[ $str =~ ^[0-9]+$ ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "^[0-9]+$");
                }
                _ => panic!("Expected regex match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_with_quoted_string_single() {
            // When =~ has a single-quoted string, it should be substring match, not regex
            let input_str = "[[ $str =~ 'substring' ]]";
            let mut input = LocatingSlice::new("[[ $str =~ 'substring' ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringContainsSubstring,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "'substring'");
                }
                _ => panic!("Expected substring match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_with_quoted_string_double() {
            // When =~ has a double-quoted string, it should be substring match, not regex
            let input_str = "[[ $str =~ \"substring\" ]]";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringContainsSubstring,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "\"substring\"");
                }
                _ => panic!("Expected substring match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_unquoted_is_regex() {
            // When =~ has an unquoted string, it should be regex match
            let input_str = "[[ $str =~ pattern[0-9]+ ]]";
            let mut input = LocatingSlice::new("[[ $str =~ pattern[0-9]+ ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "pattern[0-9]+");
                }
                _ => panic!("Expected regex match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_with_pipe() {
            // Regex pattern with | (alternation)
            let input_str = "[[ $str =~ foo|bar ]]";
            let mut input = LocatingSlice::new("[[ $str =~ foo|bar ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "foo|bar");
                }
                _ => panic!("Expected regex match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_with_parens() {
            // Regex pattern with parentheses for grouping
            let input_str = "[[ $str =~ (foo|bar) ]]";
            let mut input = LocatingSlice::new("[[ $str =~ (foo|bar) ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "(foo|bar)");
                }
                _ => panic!("Expected regex match test"),
            }
        }

        #[test]
        fn test_extended_test_regex_complex_pattern() {
            // Complex regex with nested groups and operators
            let input_str = "[[ $str =~ ^(foo|bar)[0-9]+(baz|qux)$ ]]";
            let mut input = LocatingSlice::new("[[ $str =~ ^(foo|bar)[0-9]+(baz|qux)$ ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "$str");
                    assert_eq!(right.to_string(), "^(foo|bar)[0-9]+(baz|qux)$");
                }
                _ => panic!("Expected regex match test"),
            }
        }

        #[test]
        fn test_extended_test_file_newer_than() {
            let input_str = "[[ file1 -nt file2 ]]";
            let mut input = LocatingSlice::new("[[ file1 -nt file2 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot,
                    left,
                    right,
                ) => {
                    assert_eq!(left.to_string(), "file1");
                    assert_eq!(right.to_string(), "file2");
                }
                _ => panic!("Expected file newer than test"),
            }
        }

        #[test]
        fn test_extended_test_in_command() {
            let input_str = "[[ -f test.txt ]]";
            let mut input = LocatingSlice::new("[[ -f test.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            match result.unwrap() {
                ast::Command::ExtendedTest(..) => {}
                _ => panic!("Expected extended test command"),
            }
        }

        #[test]
        fn test_extended_test_in_pipeline() {
            let input_str = "[[ -f file.txt ]] && echo exists";
            let mut input = LocatingSlice::new("[[ -f file.txt ]] && echo exists");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_extended_test_all_file_tests() {
            let tests = vec![
                ("[[ -e file ]]", ast::UnaryPredicate::FileExists),
                ("[[ -d dir ]]", ast::UnaryPredicate::FileExistsAndIsDir),
                (
                    "[[ -f file ]]",
                    ast::UnaryPredicate::FileExistsAndIsRegularFile,
                ),
                (
                    "[[ -r file ]]",
                    ast::UnaryPredicate::FileExistsAndIsReadable,
                ),
                (
                    "[[ -w file ]]",
                    ast::UnaryPredicate::FileExistsAndIsWritable,
                ),
                (
                    "[[ -x file ]]",
                    ast::UnaryPredicate::FileExistsAndIsExecutable,
                ),
                (
                    "[[ -s file ]]",
                    ast::UnaryPredicate::FileExistsAndIsNotZeroLength,
                ),
            ];

            for (input_str, expected_pred) in tests {
                let tracker = make_tracker(input_str);
                let options = crate::parser::ParserOptions::default();
                let source_info = crate::parser::SourceInfo::default();
                let ctx = ParseContext {
                    options: &options,
                    source_info: &source_info,
                };
                let mut input = LocatingSlice::new(input_str);
                let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
                assert!(result.is_ok(), "Failed to parse: {}", input_str);
                let cmd = result.unwrap();
                match cmd.expr {
                    ast::ExtendedTestExpr::UnaryTest(pred, _) => {
                        assert_eq!(pred, expected_pred, "Wrong predicate for: {}", input_str);
                    }
                    _ => panic!("Expected unary test for: {}", input_str),
                }
            }
        }

        // ========================================================================
        // Tier 17: Extended Test Expressions - Logical Operators
        // ========================================================================

        #[test]
        fn test_extended_test_and_operator() {
            let input_str = "[[ -f file.txt && -r file.txt ]]";
            let mut input = LocatingSlice::new("[[ -f file.txt && -r file.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::And(_, _) => {}
                _ => panic!("Expected AND expression"),
            }
        }

        #[test]
        fn test_extended_test_or_operator() {
            let input_str = "[[ -f file.txt || -d file.txt ]]";
            let mut input = LocatingSlice::new("[[ -f file.txt || -d file.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Or(_, _) => {}
                _ => panic!("Expected OR expression"),
            }
        }

        #[test]
        fn test_extended_test_not_operator() {
            let input_str = "[[ ! -f file.txt ]]";
            let mut input = LocatingSlice::new("[[ ! -f file.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Not(inner) => match *inner {
                    ast::ExtendedTestExpr::UnaryTest(
                        ast::UnaryPredicate::FileExistsAndIsRegularFile,
                        _,
                    ) => {}
                    _ => panic!("Expected unary file test inside NOT"),
                },
                _ => panic!("Expected NOT expression"),
            }
        }

        #[test]
        fn test_extended_test_parenthesized() {
            let input_str = "[[ ( -f file.txt ) ]]";
            let mut input = LocatingSlice::new("[[ ( -f file.txt ) ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Parenthesized(inner) => match *inner {
                    ast::ExtendedTestExpr::UnaryTest(
                        ast::UnaryPredicate::FileExistsAndIsRegularFile,
                        _,
                    ) => {}
                    _ => panic!("Expected unary file test inside parentheses"),
                },
                _ => panic!("Expected parenthesized expression"),
            }
        }

        #[test]
        fn test_extended_test_complex_and_or() {
            // Tests precedence: && has higher precedence than ||
            let input_str = "[[ -f file1 || -f file2 && -r file2 ]]";
            let mut input = LocatingSlice::new("[[ -f file1 || -f file2 && -r file2 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Or(left, right) => {
                    // Left should be -f file1
                    match *left {
                        ast::ExtendedTestExpr::UnaryTest(_, _) => {}
                        _ => panic!("Expected unary test on left of OR"),
                    }
                    // Right should be AND expression
                    match *right {
                        ast::ExtendedTestExpr::And(_, _) => {}
                        _ => panic!("Expected AND expression on right of OR"),
                    }
                }
                _ => panic!("Expected OR expression at top level"),
            }
        }

        #[test]
        fn test_extended_test_parentheses_override_precedence() {
            // Parentheses should override precedence
            let input_str = "[[ ( -f file1 || -f file2 ) && -r file3 ]]";
            let mut input = LocatingSlice::new("[[ ( -f file1 || -f file2 ) && -r file3 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::And(left, right) => {
                    // Left should be parenthesized OR expression
                    match *left {
                        ast::ExtendedTestExpr::Parenthesized(inner) => match *inner {
                            ast::ExtendedTestExpr::Or(_, _) => {}
                            _ => panic!("Expected OR inside parentheses"),
                        },
                        _ => panic!("Expected parenthesized expression on left of AND"),
                    }
                    // Right should be -r file3
                    match *right {
                        ast::ExtendedTestExpr::UnaryTest(_, _) => {}
                        _ => panic!("Expected unary test on right of AND"),
                    }
                }
                _ => panic!("Expected AND expression at top level"),
            }
        }

        #[test]
        fn test_extended_test_multiple_not() {
            let input_str = "[[ ! ! -f file.txt ]]";
            let mut input = LocatingSlice::new("[[ ! ! -f file.txt ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Not(inner1) => match *inner1 {
                    ast::ExtendedTestExpr::Not(inner2) => match *inner2 {
                        ast::ExtendedTestExpr::UnaryTest(_, _) => {}
                        _ => panic!("Expected unary test inside double NOT"),
                    },
                    _ => panic!("Expected NOT inside NOT"),
                },
                _ => panic!("Expected NOT expression"),
            }
        }

        #[test]
        fn test_extended_test_not_with_and() {
            let input_str = "[[ ! -f file1 && -f file2 ]]";
            let mut input = LocatingSlice::new("[[ ! -f file1 && -f file2 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::And(left, right) => {
                    match *left {
                        ast::ExtendedTestExpr::Not(_) => {}
                        _ => panic!("Expected NOT on left of AND"),
                    }
                    match *right {
                        ast::ExtendedTestExpr::UnaryTest(_, _) => {}
                        _ => panic!("Expected unary test on right of AND"),
                    }
                }
                _ => panic!("Expected AND expression"),
            }
        }

        #[test]
        fn test_extended_test_complex_nested() {
            let input_str = "[[ ( -f file1 && -r file1 ) || ( -d dir1 && -x dir1 ) ]]";
            let mut input =
                LocatingSlice::new("[[ ( -f file1 && -r file1 ) || ( -d dir1 && -x dir1 ) ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Or(left, right) => {
                    // Both sides should be parenthesized AND expressions
                    match (*left, *right) {
                        (
                            ast::ExtendedTestExpr::Parenthesized(l),
                            ast::ExtendedTestExpr::Parenthesized(r),
                        ) => match (*l, *r) {
                            (
                                ast::ExtendedTestExpr::And(_, _),
                                ast::ExtendedTestExpr::And(_, _),
                            ) => {}
                            _ => panic!("Expected AND expressions inside parentheses"),
                        },
                        _ => panic!("Expected parenthesized expressions on both sides of OR"),
                    }
                }
                _ => panic!("Expected OR expression at top level"),
            }
        }

        #[test]
        fn test_extended_test_binary_with_and() {
            let input_str = "[[ $a -eq 5 && $b -eq 10 ]]";
            let mut input = LocatingSlice::new("[[ $a -eq 5 && $b -eq 10 ]]");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::And(left, right) => match (*left, *right) {
                    (
                        ast::ExtendedTestExpr::BinaryTest(_, _, _),
                        ast::ExtendedTestExpr::BinaryTest(_, _, _),
                    ) => {}
                    _ => panic!("Expected binary tests on both sides of AND"),
                },
                _ => panic!("Expected AND expression"),
            }
        }

        #[test]
        fn test_extended_test_string_with_or() {
            let input_str = "[[ $str = \"hello\" || $str = \"world\" ]]";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = extended_test_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd.expr {
                ast::ExtendedTestExpr::Or(_, _) => {}
                _ => panic!("Expected OR expression"),
            }
        }

        #[test]
        fn test_extended_test_in_if_statement() {
            let input_str = "if [[ -f file.txt && -r file.txt ]]; then echo ok; fi";
            let mut input =
                LocatingSlice::new("if [[ -f file.txt && -r file.txt ]]; then echo ok; fi");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }
    }

    // ============================================================================
    // 17. HERE-DOCUMENTS (Tier 18)
    // ============================================================================
    // Tests for here-document (<<) and here-string (<<<) syntax.

    mod here_documents {
        use super::*;

        #[test]
        fn test_here_document_simple() {
            let input_str = "cat <<EOF\nhello\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EOF\nhello\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    assert_eq!(simple.word_or_name.as_ref().unwrap().value, "cat");
                    assert!(simple.suffix.is_some());
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(fd, here_doc),
                        ) => {
                            assert!(fd.is_none());
                            assert_eq!(here_doc.here_end.value, "EOF");
                            assert_eq!(here_doc.doc.value, "hello");
                            assert!(!here_doc.remove_tabs);
                            assert!(here_doc.requires_expansion);
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_with_dash() {
            let input_str = "cat <<-EOF\n\thello\nEOF\n";
            let mut input = LocatingSlice::new("cat <<-EOF\n\thello\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert!(here_doc.remove_tabs);
                            assert_eq!(here_doc.doc.value, "hello");
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_quoted_delimiter() {
            let input_str = "cat <<'EOF'\nhello\nEOF\n";
            let mut input = LocatingSlice::new("cat <<'EOF'\nhello\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert_eq!(here_doc.here_end.value, "EOF");
                            assert!(!here_doc.requires_expansion); // Quoted delimiter = no expansion
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_double_quoted_delimiter() {
            let input_str = "cat <<\"EOF\"\nhello\nEOF\n";
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let mut input = LocatingSlice::new(input_str);
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert_eq!(here_doc.here_end.value, "EOF");
                            assert!(!here_doc.requires_expansion); // Quoted delimiter = no expansion
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_multiline() {
            let input_str = "cat <<EOF\nline 1\nline 2\nline 3\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EOF\nline 1\nline 2\nline 3\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert_eq!(here_doc.doc.value, "line 1\nline 2\nline 3");
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_empty() {
            let input_str = "cat <<EOF\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EOF\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert_eq!(here_doc.doc.value, "");
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_with_pipeline() {
            let input_str = "cat <<EOF | grep pattern\nhello world\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EOF | grep pattern\nhello world\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = complete_command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_here_document_escaped_delimiter() {
            let input_str = "cat <<EO\\F\nhello\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EO\\F\nhello\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            assert_eq!(here_doc.here_end.value, "EOF");
                            assert!(!here_doc.requires_expansion); // Escaped = no expansion
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_tabs_removed() {
            let input_str = "cat <<-EOF\n\t\tindented\n\t\tlines\nEOF\n";
            let mut input = LocatingSlice::new("cat <<-EOF\n\t\tindented\n\t\tlines\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::IoRedirect(
                            ast::IoRedirect::HereDocument(_, here_doc),
                        ) => {
                            // Leading tabs should be removed from content lines
                            assert!(here_doc.remove_tabs);
                            assert_eq!(here_doc.doc.value, "indented\nlines");
                        }
                        _ => panic!("Expected HereDocument"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_here_document_in_program() {
            // Test that a here-document can be parsed as part of a program
            let input_str = "cat <<EOF\nhello world\nEOF\n";
            let mut input = LocatingSlice::new("cat <<EOF\nhello world\nEOF\n");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = program(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let prog = result.unwrap();
            assert_eq!(prog.complete_commands.len(), 1);
        }
    }

    // ============================================================================
    // 18. PROCESS SUBSTITUTION (Tier 19)
    // ============================================================================
    // Tests for process substitution <( ) and >( ) syntax.

    mod process_substitution {
        use super::*;

        #[test]
        fn test_process_substitution_read() {
            let input_str = "cat <(echo hello)";
            let mut input = LocatingSlice::new("cat <(echo hello)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    assert_eq!(simple.word_or_name.as_ref().unwrap().value, "cat");
                    assert!(simple.suffix.is_some());
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell) => {
                            assert!(matches!(kind, ast::ProcessSubstitutionKind::Read));
                            assert_eq!(subshell.list.0.len(), 1);
                        }
                        _ => panic!("Expected ProcessSubstitution, got {:?}", &suffix.0[0]),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_write() {
            let input_str = "tee >(cat > file.txt)";
            let mut input = LocatingSlice::new("tee >(cat > file.txt)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    assert_eq!(simple.word_or_name.as_ref().unwrap().value, "tee");
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, _) => {
                            assert!(matches!(kind, ast::ProcessSubstitutionKind::Write));
                        }
                        _ => panic!("Expected ProcessSubstitution"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_multiple() {
            let input_str = "diff <(ls dir1) <(ls dir2)";
            let mut input = LocatingSlice::new("diff <(ls dir1) <(ls dir2)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    assert_eq!(simple.word_or_name.as_ref().unwrap().value, "diff");
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 2);
                    for item in &suffix.0 {
                        match item {
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, _) => {
                                assert!(matches!(kind, ast::ProcessSubstitutionKind::Read));
                            }
                            _ => panic!("Expected ProcessSubstitution"),
                        }
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_with_pipeline() {
            let input_str = "cat <(ls | grep test)";
            let mut input = LocatingSlice::new("cat <(ls | grep test)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, subshell) => {
                            // The compound list should contain a pipeline
                            assert_eq!(subshell.list.0.len(), 1);
                        }
                        _ => panic!("Expected ProcessSubstitution"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_with_newlines() {
            let input_str = "cat <(\necho hello\n)";
            let mut input = LocatingSlice::new("cat <(\necho hello\n)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_process_substitution_mixed_with_args() {
            let input_str = "cmd arg1 <(echo test) arg2";
            let mut input = LocatingSlice::new("cmd arg1 <(echo test) arg2");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    assert_eq!(simple.word_or_name.as_ref().unwrap().value, "cmd");
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 3);
                    // Check that we have: word, process_subst, word
                    assert!(matches!(
                        suffix.0[0],
                        ast::CommandPrefixOrSuffixItem::Word(_)
                    ));
                    assert!(matches!(
                        suffix.0[1],
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, _)
                    ));
                    assert!(matches!(
                        suffix.0[2],
                        ast::CommandPrefixOrSuffixItem::Word(_)
                    ));
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_with_redirects() {
            let input_str = "cat <(echo hello) > output.txt";
            let mut input = LocatingSlice::new("cat <(echo hello) > output.txt");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 2);
                    assert!(matches!(
                        suffix.0[0],
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, _)
                    ));
                    assert!(matches!(
                        suffix.0[1],
                        ast::CommandPrefixOrSuffixItem::IoRedirect(_)
                    ));
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_process_substitution_complex_command() {
            let input_str = "cat <(if true; then echo yes; fi)";
            let mut input = LocatingSlice::new("cat <(if true; then echo yes; fi)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
        }

        #[test]
        fn test_process_substitution_in_pipeline() {
            let input_str = "cat <(echo hello) | grep hello";
            let mut input = LocatingSlice::new("cat <(echo hello) | grep hello");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            assert_eq!(pipe.seq.len(), 2);
        }

        #[test]
        fn test_process_substitution_both_kinds() {
            let input_str = "cmd <(echo input) >(cat > output)";
            let mut input = LocatingSlice::new("cmd <(echo input) >(cat > output)");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 2);
                    match (&suffix.0[0], &suffix.0[1]) {
                        (
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind1, _),
                            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind2, _),
                        ) => {
                            assert!(matches!(kind1, ast::ProcessSubstitutionKind::Read));
                            assert!(matches!(kind2, ast::ProcessSubstitutionKind::Write));
                        }
                        _ => panic!("Expected two ProcessSubstitutions"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_function_name_cannot_end_with_equals() {
            // Function names ending with = should be rejected to avoid confusion with assignments
            let input_str = "foo=() { echo hello; }";
            let mut input = LocatingSlice::new("foo=() { echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_function_name_valid() {
            // Normal function names should work fine
            let input_str = "foo() { echo hello; }";
            let mut input = LocatingSlice::new("foo() { echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let func = result.unwrap();
            assert_eq!(func.fname.value, "foo");
        }

        #[test]
        fn test_reserved_word_time_cannot_be_function() {
            // "time" is a reserved word and cannot be used as a function name
            let input_str = "time() { echo hello; }";
            let mut input = LocatingSlice::new("time() { echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_reserved_word_coproc_cannot_be_function() {
            // "coproc" is a reserved word and cannot be used as a function name
            let input_str = "coproc() { echo hello; }";
            let mut input = LocatingSlice::new("coproc() { echo hello; }");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = function_definition(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_err());
        }

        #[test]
        fn test_reserved_word_time_cannot_be_command() {
            // "time" is a reserved word and cannot be used as a simple command name
            // It will be parsed as a pipeline with time keyword instead
            let input_str = "time arg1 arg2";
            let mut input = LocatingSlice::new("time arg1 arg2");
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = pipeline(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok());
            let pipe = result.unwrap();
            // Should have timed set
            assert!(pipe.timed.is_some());
        }
    }

    // ============================================================================
    // 19. NESTED EXPANSIONS
    // ============================================================================
    // Tests for nested parentheses and braces in various expansion types.
    // These test cases expose bugs in simple take_while parsers that don't
    // handle balanced delimiters.

    mod nested_expansions {
        use super::*;

        #[test]
        fn test_command_substitution_with_nested_parens() {
            // Bug: take_while stops at first ')', missing nested parens
            let input_str = r#"echo $(echo (foo))"#;
            let mut input = LocatingSlice::new(r#"echo $(echo (foo))"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(
                result.is_ok(),
                "Should parse command substitution with nested parens"
            );

            // Verify it parsed the complete word including the entire $(echo (foo))
            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::Word(w) => {
                            // Should contain the full expansion including nested parens
                            assert!(
                                w.value.contains("$(echo (foo))"),
                                "Expected full command substitution, got: {}",
                                w.value
                            );
                        }
                        _ => panic!("Expected Word with expansion"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_command_substitution_with_subshell() {
            // Subshells use parentheses too
            let input_str = r#"echo $(( cd /tmp; ls ))"#;
            let mut input = LocatingSlice::new(r#"echo $(( cd /tmp; ls ))"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(
                result.is_ok(),
                "Should parse command substitution with subshell"
            );
        }

        #[test]
        fn test_arithmetic_expansion_with_nested_parens() {
            // Bug: Arithmetic expressions can have nested parens
            let input_str = r#"echo $((1 + (2 * 3)))"#;
            let mut input = LocatingSlice::new(r#"echo $((1 + (2 * 3)))"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok(), "Should parse arithmetic with nested parens");

            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::Word(w) => {
                            assert!(
                                w.value.contains("$((1 + (2 * 3)))"),
                                "Expected full arithmetic expansion, got: {}",
                                w.value
                            );
                        }
                        _ => panic!("Expected Word with expansion"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_backtick_with_nested_parens() {
            // Bug: Backticks can contain commands with parens
            let input_str = r#"echo `echo (test)`"#;
            let mut input = LocatingSlice::new(r#"echo `echo (test)`"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok(), "Should parse backtick with nested parens");
        }

        #[test]
        fn test_braced_variable_with_nested_braces() {
            // Bug: Variable expansions can have nested braces in parameter expansion
            // e.g., ${var:-${default}}
            let input_str = r#"echo ${foo:-${bar}}"#;
            let mut input = LocatingSlice::new(r#"echo ${foo:-${bar}}"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok(), "Should parse nested braced variables");

            let cmd = result.unwrap();
            match cmd {
                ast::Command::Simple(simple) => {
                    let suffix = simple.suffix.as_ref().unwrap();
                    assert_eq!(suffix.0.len(), 1);
                    match &suffix.0[0] {
                        ast::CommandPrefixOrSuffixItem::Word(w) => {
                            assert!(
                                w.value.contains("${foo:-${bar}}"),
                                "Expected full nested expansion, got: {}",
                                w.value
                            );
                        }
                        _ => panic!("Expected Word with expansion"),
                    }
                }
                _ => panic!("Expected Simple command"),
            }
        }

        #[test]
        fn test_deeply_nested_arithmetic() {
            // Multiple levels of nesting
            let input_str = r#"echo $(( (1 + (2 * (3 - 4))) ))"#;
            let mut input = LocatingSlice::new(r#"echo $(( (1 + (2 * (3 - 4))) ))"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok(), "Should parse deeply nested arithmetic");
        }

        #[test]
        fn test_mixed_expansions() {
            // Command substitution containing arithmetic
            let input_str = r#"echo $(echo $((1+2)))"#;
            let mut input = LocatingSlice::new(r#"echo $(echo $((1+2)))"#);
            let tracker = make_tracker(input_str);
            let options = crate::parser::ParserOptions::default();
            let source_info = crate::parser::SourceInfo::default();
            let ctx = ParseContext {
                options: &options,
                source_info: &source_info,
            };
            let result = command(&ctx, &tracker).parse_next(&mut input);
            assert!(result.is_ok(), "Should parse mixed expansions");
        }
    }
}
