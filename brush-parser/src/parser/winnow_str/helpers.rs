use winnow::combinator::{alt, eof, fail, peek, preceded, repeat, terminated};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::{Checkpoint, Offset, Stream};
use winnow::token::take_while;

use crate::ast::SeparatorOperator;

use super::types::StrStream;

// ============================================================================
// Helper: Byte-to-Character conversion for LocatingSlice<&str>
// ============================================================================

/// Take a slice from input using byte offset from checkpoints.
///
/// Winnow's `offset_from()` returns byte offsets, but `take()` expects character counts.
/// This helper correctly handles multi-byte UTF-8 by converting bytes to characters.
pub(super) fn take_slice_from_checkpoints<'a>(
    input: &mut StrStream<'a>,
    start: &Checkpoint<<&'a str as Stream>::Checkpoint, StrStream<'a>>,
) -> ModalResult<&'a str> {
    let end = input.checkpoint();
    let consumed_bytes = end.offset_from(start);

    input.reset(start);
    let bytes = input.as_bytes();
    let result = std::str::from_utf8(&bytes[..consumed_bytes])
        .map_err(|_| winnow::error::ErrMode::Backtrack(ContextError::default()))?;

    let consumed_chars = result.chars().count();
    winnow::token::take(consumed_chars).parse_next(input)
}

// ============================================================================
// Tier 0: Character-level parsers (leaf functions)
// ============================================================================

/// Helper: Peek at next 1-2 operator characters for dispatch
pub(super) fn peek_op2<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    peek(winnow::token::take_while(1..=2, |c: char| {
        matches!(c, '<' | '>' | '&' | '|')
    }))
}

/// Helper: Peek at next 2-3 operator characters for case terminators
pub(super) fn peek_op3<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    peek(winnow::token::take_while(2..=3, |c: char| {
        matches!(c, ';' | '&')
    }))
}

/// Helper: Peek at first character for `word_part` dispatch
pub(super) fn peek_char<'a>() -> impl ModalParser<StrStream<'a>, char, ContextError> {
    peek(winnow::token::any)
}

/// Parse an extended glob pattern: @(...), +(...), *(...), ?(...), !(...)
/// Returns the entire pattern including the prefix and parentheses
pub(super) fn extglob_pattern<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        // Match the prefix character (@, !, ?, +, *)
        let _prefix_char = winnow::token::one_of(['@', '!', '?', '+', '*']).parse_next(input)?;

        // Use the helper to parse balanced parens starting from the '('
        // This returns the consumed slice including the parens
        let balanced =
            parse_balanced_delimiters("(", Some('('), ')', 1, false, false).parse_next(input)?;

        // Total character count: 1 for prefix + chars in balanced content
        let char_count = 1 + balanced.chars().count();

        // Reset and take the full pattern
        input.reset(&start);
        winnow::token::take(char_count).parse_next(input)
    }
}

// ============================================================================
// Helper: Quote Skipping Parsers
// ============================================================================

/// Skip the content of a single-quoted string, assuming the opening quote was already consumed.
/// Returns the content (without quotes) followed by the closing quote.
pub(super) fn skip_single_quoted_content<'a>()
-> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    (take_while(0.., |c: char| c != '\''), '\'').take()
}

/// Skip the content of a double-quoted string, assuming the opening quote was already consumed.
/// Handles backslash escapes (\" and \\). Returns the content (without opening quote) followed by
/// the closing quote.
pub(super) fn skip_double_quoted_content<'a>()
-> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();
        let mut char_count: usize = 0;

        loop {
            match next_char(input) {
                Ok('"') => {
                    char_count += 1;
                    break;
                }
                Ok('\\') => {
                    let _ = next_char(input);
                    char_count += 2;
                }
                Err(_) => {
                    return fail.parse_next(input);
                }
                Ok(_) => {
                    char_count += 1;
                }
            }
        }

        input.reset(&start);
        winnow::token::take(char_count).parse_next(input)
    }
}

// ============================================================================
// Helper: Balanced Delimiter Parsing
// ============================================================================

/// Read the next character from the stream, returning a winnow `ErrMode` result.
/// This is a convenience wrapper that avoids repeating the full turbofish type.
#[inline]
fn next_char(input: &mut StrStream<'_>) -> Result<char, winnow::error::ErrMode<ContextError>> {
    winnow::token::any::<_, winnow::error::ErrMode<ContextError>>.parse_next(input)
}

/// Peek at the next character without consuming, checking if it's one of the
/// shell delimiter characters that terminate a word (for keyword detection).
fn peek_is_delimiter(input: &mut StrStream<'_>) -> bool {
    winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>([
        ' ', '\t', '\n', ';', '&', '|', '<', '>', '(', ')', '{', '}',
    ]))
    .parse_next(input)
    .is_ok()
        || input.is_empty()
}

/// Try to match a keyword suffix starting from the current position.
///
/// After seeing the first character of a potential keyword (e.g., 'c' for "case"),
/// read the remaining identifier characters and check if they form a delimited keyword.
///
/// Returns `true` if the suffix matches and is followed by a delimiter, consuming those
/// characters. Returns `false` and resets the stream otherwise.
fn try_keyword_suffix(input: &mut StrStream<'_>, expected_suffix: &str) -> bool {
    let checkpoint = input.checkpoint();
    if let Ok(rest) = winnow::token::take_while::<_, _, ContextError>(0.., |c: char| {
        c.is_alphanumeric() || c == '_'
    })
    .parse_next(input)
    {
        if peek_is_delimiter(input) && rest == expected_suffix {
            return true;
        }
    }
    input.reset(&checkpoint);
    false
}

// ---------------------------------------------------------------------------
// Case statement tracker
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum CaseState {
    #[default]
    NotInCase,
    AfterCase,
    AfterIn,
    InPattern,
    InBody,
}

/// Tracks `case ... esac` nesting inside balanced delimiters so that `)` is
/// correctly interpreted as a pattern separator rather than a close delimiter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct CaseTracker {
    state: CaseState,
    depth: usize,
}

impl CaseTracker {
    /// Called when `close_char` is encountered. Returns `true` if this `)` is
    /// part of a case pattern (and should NOT decrement the delimiter depth).
    const fn on_close_delimiter(&mut self) -> bool {
        if matches!(self.state, CaseState::AfterIn | CaseState::InPattern) {
            self.state = CaseState::InBody;
            true
        } else {
            false
        }
    }

    /// Process a character that might be part of a case keyword.
    /// Returns `true` if the character was consumed as part of keyword detection.
    fn try_update(&mut self, ch: char, input: &mut StrStream<'_>) -> bool {
        match ch {
            'c' if self.state == CaseState::NotInCase => {
                if try_keyword_suffix(input, "ase") {
                    self.state = CaseState::AfterCase;
                    self.depth += 1;
                    return true;
                }
            }
            'i' if self.state == CaseState::AfterCase => {
                if try_keyword_suffix(input, "n") {
                    self.state = CaseState::AfterIn;
                    return true;
                }
            }
            'e' if self.depth > 0 => {
                if try_keyword_suffix(input, "sac") {
                    self.depth = self.depth.saturating_sub(1);
                    self.state = if self.depth == 0 {
                        CaseState::NotInCase
                    } else {
                        CaseState::AfterIn
                    };
                    return true;
                }
            }
            ';' if self.state == CaseState::InBody => {
                let checkpoint = input.checkpoint();
                // ;; or ;;&
                if next_char(input) == Ok(';') {
                    let _ =
                        winnow::combinator::opt(winnow::token::one_of::<_, _, ContextError>('&'))
                            .parse_next(input);
                    self.state = CaseState::AfterIn;
                    return true;
                }
                // ;& (fallthrough)
                input.reset(&checkpoint);
                if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('&'))
                    .parse_next(input)
                    .is_ok()
                {
                    let _ = next_char(input);
                    self.state = CaseState::AfterIn;
                    return true;
                }
                input.reset(&checkpoint);
            }
            _ => {}
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Heredoc tracker
// ---------------------------------------------------------------------------

/// Tracks pending heredocs inside balanced delimiters: their delimiters, whether
/// to strip leading tabs, and whether we're currently consuming heredoc body content.
struct HeredocTracker {
    pending: Vec<(String, bool)>,
    in_body: bool,
}

impl HeredocTracker {
    const fn new() -> Self {
        Self {
            pending: Vec::new(),
            in_body: false,
        }
    }

    /// After a newline, enter heredoc body mode if there are pending heredocs.
    const fn on_newline(&mut self) {
        if !self.pending.is_empty() {
            self.in_body = true;
        }
    }

    /// Try to consume a heredoc body line. Returns `Ok(true)` if a line was
    /// consumed (either as a delimiter match or as body content), `Ok(false)`
    /// if we're not in heredoc body mode, or `Err` on parse failure (e.g. EOF
    /// in heredoc body).
    fn try_consume_body_line(&mut self, input: &mut StrStream<'_>) -> ModalResult<bool> {
        if !self.in_body || self.pending.is_empty() {
            return Ok(false);
        }

        let (delimiter, remove_tabs) = &self.pending[0];

        if *remove_tabs {
            let _: ModalResult<&str> = winnow::token::take_while(0.., '\t').parse_next(input);
        }

        let checkpoint = input.checkpoint();
        if let Ok(line_content) =
            winnow::token::take_while::<_, _, ContextError>(0.., |c| c != '\n').parse_next(input)
        {
            if line_content == delimiter {
                self.pending.remove(0);
                let _ = next_char(input);
                if self.pending.is_empty() {
                    self.in_body = false;
                }
                return Ok(true);
            }
        }
        input.reset(&checkpoint);

        loop {
            match next_char(input) {
                Ok('\n') => break,
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    /// Add a new pending heredoc.
    fn push(&mut self, delimiter: String, remove_tabs: bool) {
        if !delimiter.is_empty() {
            self.pending.push((delimiter, remove_tabs));
        }
    }
}

// ---------------------------------------------------------------------------
// Bracket expression consumer
// ---------------------------------------------------------------------------

/// After a `$` has been consumed inside `try_consume_bracket_expression`, try
/// to skip a `$`-expansion (`$(...)`, `$((...))`, `${...}`). Returns `Ok(true)`
/// if an expansion was consumed, `Ok(false)` if not (stream is reset to just
/// after the `$`).
fn skip_dollar_expansion_in_bracket(
    input: &mut StrStream<'_>,
    close_char: char,
) -> Result<bool, winnow::error::ErrMode<ContextError>> {
    match next_char(input) {
        Ok('(') => {
            let checkpoint = input.checkpoint();
            if winnow::token::one_of::<_, _, ContextError>('(')
                .parse_next(input)
                .is_ok()
            {
                if parse_balanced_delimiters("", Some('('), ')', 2, false, false)
                    .parse_next(input)
                    .is_err()
                {
                    input.reset(&checkpoint);
                    if parse_balanced_delimiters("", Some('('), ')', 1, true, true)
                        .parse_next(input)
                        .is_err()
                    {
                        return Ok(false);
                    }
                }
            } else if parse_balanced_delimiters("", Some('('), ')', 1, true, true)
                .parse_next(input)
                .is_err()
            {
                return Ok(false);
            }
            Ok(true)
        }
        Ok('{') => {
            if parse_balanced_delimiters("", Some('{'), '}', 1, false, false)
                .parse_next(input)
                .is_err()
            {
                return Ok(false);
            }
            Ok(true)
        }
        Ok(c) if c == close_char || c == ']' => Ok(false),
        Ok(_) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Inside a bracket expression `[...]`, skip a POSIX bracket expression class
/// like `[:class:]`, `[.coll.]`, or `[=equiv=]`. The `[` and the class
/// delimiter character have already been consumed; `end_char` is one of
/// `:`, `.`, or `=`.
fn skip_bracket_class(input: &mut StrStream<'_>, end_char: char) -> ModalResult<()> {
    loop {
        match next_char(input) {
            Ok(c) if c == end_char => {
                if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>(']'))
                    .parse_next(input)
                    .is_ok()
                {
                    let _ = next_char(input);
                    return Ok(());
                }
            }
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }
}

/// Attempt to consume a complete bracket expression `[...]` from the input,
/// assuming the opening `[` has already been consumed.
///
/// Bracket expressions appear in glob patterns and parameter expansion patterns.
/// Inside them, `{`, `}`, `(`, `)` are literal characters. By consuming the whole
/// expression, we prevent those characters from affecting the delimiter depth
/// tracking in `parse_balanced_delimiters`.
///
/// Special cases handled:
/// - `]` as the first character after `[` is literal (e.g., `[]abc]`)
/// - `[^...]` or `[!...]` — negated bracket expression
/// - `[:class:]`, `[=equiv=]`, `[.coll.]` — POSIX bracket expression classes
/// - Backslash escapes inside the expression
/// - Single/double-quoted strings and `$`-expansions (their `]` chars are not
///   treated as closing the bracket expression)
///
/// Returns `Ok` if a complete `[...]` was consumed, `Err` if no closing `]`
/// was found (meaning the `[` was just a literal character).
///
/// `close_char` is the delimiter we're scanning within (e.g., `}` for `${...}`).
/// We will NOT consume past `close_char` — if we hit it before finding `]`,
/// the `[` was just a literal character.
fn try_consume_bracket_expression(
    input: &mut StrStream<'_>,
    close_char: char,
) -> Result<(), winnow::error::ErrMode<ContextError>> {
    let first = next_char(input);
    match first {
        Ok(']' | '^' | '!') => {}
        Ok(c) if c == close_char => {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }
        Ok('\'') => {
            let _ = skip_single_quoted_content().parse_next(input)?;
        }
        Ok('"') => {
            let _ = skip_double_quoted_content().parse_next(input)?;
        }
        Ok('$') => {
            let checkpoint = input.checkpoint();
            if !skip_dollar_expansion_in_bracket(input, close_char)? {
                input.reset(&checkpoint);
            }
        }
        Ok(_) => {}
        Err(e) => return Err(e),
    }

    loop {
        match next_char(input) {
            Ok(']') => return Ok(()),
            Ok(c) if c == close_char => {
                return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
            }
            Ok('[') => {
                let checkpoint = input.checkpoint();
                let class_char = next_char(input);
                match class_char {
                    Ok(end_char @ (':' | '.' | '=')) => {
                        skip_bracket_class(input, end_char)?;
                    }
                    Ok(_) => {
                        input.reset(&checkpoint);
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok('\\') => {
                let _ = next_char(input);
            }
            Ok('\'') => {
                let _ = skip_single_quoted_content().parse_next(input)?;
            }
            Ok('"') => {
                let _ = skip_double_quoted_content().parse_next(input)?;
            }
            Ok('$') => {
                let checkpoint = input.checkpoint();
                if !skip_dollar_expansion_in_bracket(input, close_char)? {
                    input.reset(&checkpoint);
                }
            }
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }
}

/// After encountering `$`, handle nested expansions: `$(...)`, `${...}`, `$((...))`.
///
/// Peeks at the next character and dispatches accordingly. If no expansion is
/// recognized (e.g., bare `$` before a letter), resets the stream to the pre-`$`
/// checkpoint (the caller must supply it).
fn handle_dollar_expansion<'a>(
    input: &mut StrStream<'a>,
    checkpoint: &Checkpoint<<&'a str as Stream>::Checkpoint, StrStream<'a>>,
) -> ModalResult<()> {
    if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('('))
        .parse_next(input)
        .is_ok()
    {
        let _ = next_char(input)?;
        if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('('))
            .parse_next(input)
            .is_ok()
        {
            let _ = next_char(input)?;
            let _ =
                parse_balanced_delimiters("", Some('('), ')', 2, false, false).parse_next(input)?;
        } else {
            let _ =
                parse_balanced_delimiters("", Some('('), ')', 1, true, true).parse_next(input)?;
        }
    } else if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('{'))
        .parse_next(input)
        .is_ok()
    {
        let _ = next_char(input)?;
        let _ = parse_balanced_delimiters("", Some('{'), '}', 1, false, false).parse_next(input)?;
    } else {
        input.reset(checkpoint);
    }
    Ok(())
}

/// After encountering `<` (when heredocs are allowed), detect `<<` heredoc or
/// `<<<` here-string. On success, pushes the heredoc onto `heredocs`.
/// Returns `true` if the `<` was consumed as part of a heredoc/here-string,
/// `false` if it was just a literal `<` (stream is reset to `checkpoint`).
fn handle_heredoc_open<'a>(
    input: &mut StrStream<'a>,
    checkpoint: &Checkpoint<<&'a str as Stream>::Checkpoint, StrStream<'a>>,
    heredocs: &mut HeredocTracker,
) -> ModalResult<bool> {
    if next_char(input) != Ok('<') {
        input.reset(checkpoint);
        return Ok(false);
    }
    if winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('<'))
        .parse_next(input)
        .is_ok()
    {
        let _ = next_char(input);
        return Ok(true);
    }
    let remove_tabs = winnow::combinator::peek(winnow::token::one_of::<_, _, ContextError>('-'))
        .parse_next(input)
        .is_ok();
    if remove_tabs {
        let _ = next_char(input);
    }
    let delimiter = parse_heredoc_delimiter_in_balanced(input)?;
    heredocs.push(delimiter, remove_tabs);
    Ok(true)
}

/// Returns the full slice including opening and closing delimiters
///
/// # Parameters
/// - `prefix`: The opening delimiter(s) to match first (e.g., "$(", "${", backtick)
/// - `open_char`: Character that increases depth (e.g., '(' or '{'), or None for backticks
/// - `close_char`: Character that decreases depth (e.g., ')' or '}' or backtick)
/// - `initial_depth`: Starting depth (e.g., 1 for most, 2 for arithmetic `$((`)
/// - `allow_comments`: Whether to recognize `#` as starting a comment (true for command substitutions)
/// - `allow_heredocs`: Whether to recognize heredocs (true for command substitutions)
///
/// # Examples
/// - Command substitution: `parse_balanced_delimiters("$(", Some('('), ')', 1, true, true)`
/// - Arithmetic: `parse_balanced_delimiters("$((", Some('('), ')', 2, false, false)`
/// - Braced variable: `parse_balanced_delimiters("${", Some('{'), '}', 1, false, false)`
/// - Backtick: `parse_balanced_delimiters("`", None, '`', 1, true, true)`
pub(super) fn parse_balanced_delimiters<'a>(
    prefix: &'a str,
    open_char: Option<char>,
    close_char: char,
    initial_depth: usize,
    allow_comments: bool,
    allow_heredocs: bool,
) -> impl ModalParser<StrStream<'a>, &'a str, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        winnow::token::literal(prefix).parse_next(input)?;

        let mut depth = initial_depth;
        let mut at_comment_start = allow_comments;
        let mut heredocs = HeredocTracker::new();
        let mut case = CaseTracker::default();

        tracing::debug!("parse_balanced_delimiters: starting, prefix={:?}", prefix);

        while depth > 0 {
            if heredocs.try_consume_body_line(input)? {
                continue;
            }

            match next_char(input) {
                Ok(ch) if Some(ch) == open_char => {
                    depth += 1;
                    at_comment_start = false;
                }
                Ok(ch) if ch == close_char => {
                    if !case.on_close_delimiter() {
                        depth -= 1;
                    }
                    at_comment_start = false;
                }
                Ok(ch) if case.try_update(ch, input) => {
                    at_comment_start = false;
                }
                Ok('\\') => {
                    let _ = next_char(input);
                    at_comment_start = false;
                }
                Ok('[') => {
                    let checkpoint = input.checkpoint();
                    if try_consume_bracket_expression(input, close_char).is_err() {
                        input.reset(&checkpoint);
                    }
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
                Ok('$') => {
                    let checkpoint = input.checkpoint();
                    handle_dollar_expansion(input, &checkpoint)?;
                    at_comment_start = false;
                }
                Ok('#') if at_comment_start => {
                    while let Ok(c) = next_char(input) {
                        if c == '\n' {
                            at_comment_start = true;
                            break;
                        }
                    }
                }
                Ok('<') if allow_heredocs && depth == initial_depth => {
                    let checkpoint = input.checkpoint();
                    if handle_heredoc_open(input, &checkpoint, &mut heredocs)? {
                        at_comment_start = false;
                    } else {
                        at_comment_start = allow_comments && matches!('<', ' ' | '\t' | '\n');
                    }
                }
                Ok('\n') => {
                    at_comment_start = allow_comments;
                    heredocs.on_newline();
                }
                Ok(ch) => {
                    at_comment_start = allow_comments && matches!(ch, ' ' | '\t' | '\n');
                }
                Err(_) => {
                    return fail.parse_next(input);
                }
            }
        }

        super::helpers::take_slice_from_checkpoints(input, &start)
    }
}

/// Parse a heredoc delimiter (the word after << or <<-)
/// Returns the delimiter string (with quotes stripped for matching)
fn parse_heredoc_delimiter_in_balanced(input: &mut StrStream<'_>) -> ModalResult<String> {
    let mut delimiter = String::new();

    let _: ModalResult<&str> =
        winnow::token::take_while(0.., |c| c == ' ' || c == '\t').parse_next(input);

    while !input.is_empty() {
        let checkpoint = input.checkpoint();

        if let Ok(_ch) =
            winnow::token::one_of::<_, _, ContextError>([' ', '\t', '\n', ')', '|', '&', ';'])
                .parse_next(input)
        {
            input.reset(&checkpoint);
            break;
        }
        input.reset(&checkpoint);

        let ch = next_char(input)?;

        match ch {
            '\'' => loop {
                match next_char(input) {
                    Ok('\'') => break,
                    Ok(c) => delimiter.push(c),
                    Err(_) => break,
                }
            },
            '"' => loop {
                match next_char(input) {
                    Ok('"') => break,
                    Ok('\\') => {
                        if let Ok(next) = next_char(input) {
                            delimiter.push(next);
                        }
                    }
                    Ok(c) => delimiter.push(c),
                    Err(_) => break,
                }
            },
            '\\' => {
                if let Ok(next) = next_char(input) {
                    delimiter.push(next);
                }
            }
            _ => {
                delimiter.push(ch);
            }
        }
    }

    Ok(delimiter)
}

/// Check if character is valid in a username for tilde expansion
/// POSIX portable filename characters: alphanumeric, dot, underscore, hyphen, plus
const fn is_username_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '_' | '-' | '+')
}

/// Parse a tilde expansion: ~, ~user, ~+, ~-, ~+N, ~-N
/// Returns the entire tilde expression as a string
pub(super) fn tilde_expansion<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
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
pub(super) fn newline<'a>() -> impl ModalParser<StrStream<'a>, char, ContextError> {
    '\n'
}

/// Parse a comment: # to end of line (not including newline)
/// Comments start with # and continue to end of line
/// The # must appear at a word boundary (start of input or after whitespace)
#[inline]
pub(super) fn comment<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    ('#', take_while(0.., |c: char| c != '\n')).void()
}

/// Parse optional whitespace and comments (spaces, tabs, and comments, but NOT newlines)
///
/// Handles both inter-token spaces, inline comments, and backslash-newline
/// continuations like: `echo hello # comment` or `cmd \<NL> arg`.
/// This is needed to separate tokens on the same line.
#[inline]
pub(super) fn spaces<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
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
pub(super) fn spaces1<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
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
pub(super) fn array_spaces<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
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
pub(super) fn linebreak<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
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
pub(super) fn newline_list<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
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
pub(super) fn separator_op<'a>() -> impl ModalParser<StrStream<'a>, SeparatorOperator, ContextError>
{
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
pub(super) fn separator<'a>()
-> impl ModalParser<StrStream<'a>, Option<SeparatorOperator>, ContextError> {
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
pub(super) fn sequential_sep<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    winnow::combinator::alt(((spaces(), ';', linebreak()).void(), newline_list().void()))
}

/// Match a specific keyword (shell reserved word)
/// Keywords must be followed by a delimiter (space, tab, newline, semicolon, etc.)
/// to avoid matching them as part of a larger word
pub(super) fn keyword<'a>(
    word: &'static str,
) -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    // Skip spaces, match the literal, then peek that a delimiter or EOF follows —
    // preventing "time" from matching inside "timestamp", etc.
    preceded(
        spaces(),
        terminated(
            winnow::token::literal(word),
            peek(alt((
                eof.void(),
                winnow::token::one_of(|c: char| {
                    c.is_whitespace()
                        || matches!(c, ';' | '&' | '|' | '<' | '>' | '(' | ')' | '{' | '}')
                })
                .void(),
            ))),
        ),
    )
}

/// Peek the first word without consuming input (for keyword dispatch)
pub(super) fn peek_first_word<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
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
pub(super) fn name<'a>() -> impl ModalParser<StrStream<'a>, String, ContextError> {
    winnow::combinator::preceded(spaces(), super::words::bare_word())
        .verify(|s: &str| is_valid_name(s))
        .map(|s: &str| s.to_string())
}

/// Parse a function name.
pub(super) fn fname<'a>() -> impl ModalParser<StrStream<'a>, String, ContextError> {
    winnow::combinator::preceded(spaces(), super::words::fname_word())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::winnow_str::types::StrStream;

    fn parse_braced(input: &str) -> Result<&str, winnow::error::ErrMode<ContextError>> {
        let mut stream = StrStream::new(input);
        parse_balanced_delimiters("${", Some('{'), '}', 1, false, false).parse_next(&mut stream)
    }

    fn parse_cmd_sub(input: &str) -> Result<&str, winnow::error::ErrMode<ContextError>> {
        let mut stream = StrStream::new(input);
        parse_balanced_delimiters("$(", Some('('), ')', 1, true, true).parse_next(&mut stream)
    }

    #[test]
    fn test_bracket_expr_with_braces_in_param_expansion() {
        let result = parse_braced("${y%%[<{().]}");
        assert!(
            result.is_ok(),
            "Bracket expr with braces should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_with_parens_in_param_expansion() {
        let result = parse_braced("${y%%[<().]}");
        assert!(
            result.is_ok(),
            "Bracket expr with parens should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_with_braces_and_star() {
        let result = parse_braced("${y%%[<{().[]*}");
        assert!(
            result.is_ok(),
            "Bracket expr with braces and star should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_with_literal_close_bracket() {
        let result = parse_braced("${y%%[]}]}");
        assert!(
            result.is_ok(),
            "Bracket expr with ] as first char should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_with_caret_in_param_expansion() {
        let result = parse_braced("${y%%[^<{}()]}");
        assert!(
            result.is_ok(),
            "Bracket expr with negated class should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_in_command_substitution() {
        let result = parse_cmd_sub("$(echo ${y%%[<{().]})");
        assert!(
            result.is_ok(),
            "Bracket expr inside cmd sub should parse: {result:?}"
        );
    }

    #[test]
    fn test_nested_param_expansion_with_bracket_expr() {
        let result = parse_braced("${y#${z%%[<{().]}}");
        assert!(
            result.is_ok(),
            "Nested param with bracket expr should parse: {result:?}"
        );
    }

    #[test]
    fn test_simple_param_expansion_still_works() {
        let result = parse_braced("${y%%pattern}");
        assert!(
            result.is_ok(),
            "Simple param expansion should still parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_does_not_affect_real_nesting() {
        let result = parse_braced("${y#${z}}");
        assert!(
            result.is_ok(),
            "Nested braces without bracket expr should parse: {result:?}"
        );
    }

    #[test]
    fn test_literal_bracket_in_param_replacement() {
        let result = parse_braced("${y//a/[}");
        assert!(
            result.is_ok(),
            "Literal [ in replacement should parse: {result:?}"
        );
    }

    #[test]
    fn test_escaped_bracket_in_param_pattern() {
        let result = parse_braced("${y//\\[/\\[}");
        assert!(
            result.is_ok(),
            "Escaped [ in pattern and replacement should parse: {result:?}"
        );
    }

    #[test]
    fn test_escaped_bracket_with_backslash_replacement() {
        let result = parse_braced("${y//\\[/\\\\[}");
        assert!(
            result.is_ok(),
            "Escaped [ with backslash replacement should parse: {result:?}"
        );
    }

    #[test]
    fn test_bracket_expr_not_confused_by_faraway_close_bracket() {
        let result = parse_braced("${y//a/[}x]");
        assert!(
            result.is_ok(),
            "Literal [ should not scan past close_char: {result:?}"
        );
    }

    #[test]
    fn test_try_consume_bracket_expression_basic() {
        let mut input = StrStream::new("abc]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should consume [abc]: {result:?}");
        // Should have consumed up to and including ]
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    #[test]
    fn test_try_consume_bracket_expression_stops_at_close_char() {
        let mut input = StrStream::new("a}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(
            result.is_err(),
            "Should fail when close_char found before ]"
        );
    }

    #[test]
    fn test_try_consume_bracket_expression_negated() {
        let mut input = StrStream::new("^abc]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should consume [^abc]: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    #[test]
    fn test_try_consume_bracket_expression_literal_close_bracket() {
        let mut input = StrStream::new("]abc]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should consume []abc]: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    #[test]
    fn test_try_consume_bracket_expression_with_escape() {
        let mut input = StrStream::new("\\a]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should consume [\\a]: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    /// Regression test: `]` inside `${arr[@]}` within a double-quoted string
    /// inside `$([ ... ])` must not be consumed by the bracket expression scanner.
    #[test]
    fn test_bracket_expr_does_not_match_close_bracket_inside_quotes_in_test_cmd() {
        // $([ "${arr[@]}" = "" ]) — the [ starts a test command, not a bracket expr
        let result = parse_cmd_sub("$([ \"${arr[@]}\" = \"\" ])");
        assert!(
            result.is_ok(),
            "Cmd sub with test command containing ${{arr[@]}} should parse: {result:?}"
        );
    }

    /// Bracket expression scanner should skip over double-quoted strings.
    #[test]
    fn test_bracket_expr_skips_double_quoted_content() {
        // Input after consuming [: abc"def]"ghi]
        // The ] inside the quotes should be skipped
        let mut input = StrStream::new("abc\"def]\"ghi]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should skip double-quoted ]: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    /// Bracket expression scanner should skip over single-quoted strings.
    #[test]
    fn test_bracket_expr_skips_single_quoted_content() {
        // Input after consuming [: abc'def]'ghi]
        let mut input = StrStream::new("abc'def]'ghi]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should skip single-quoted ]: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }

    /// Bracket expression scanner should skip over ${...} expansions.
    #[test]
    fn test_bracket_expr_skips_braced_expansion() {
        // Input after consuming [: ${arr[@]}]
        let mut input = StrStream::new("${arr[@]}]}");
        let result = try_consume_bracket_expression(&mut input, '}');
        assert!(result.is_ok(), "Should skip ${{...}}: {result:?}");
        let remaining: &str = input.finish();
        assert_eq!(remaining, "}");
    }
}
