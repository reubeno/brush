use winnow::combinator::{alt, eof, fail, peek, preceded, repeat, terminated};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::{Checkpoint, Location, Offset, Stream};
use winnow::token::take_while;

use crate::ast::SeparatorOperator;

use super::types::StrStream;

/// Take the input consumed since `start` as one slice of it.
pub(super) fn take_slice_from_checkpoints<'a>(
    input: &mut StrStream<'a>,
    start: &Checkpoint<<&'a str as Stream>::Checkpoint, StrStream<'a>>,
) -> &'a str {
    let consumed = input.offset_from(start);
    input.reset(start);
    input.next_slice(consumed)
}

// ============================================================================
// Character-level parsers (leaf functions)
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
        // The prefix (@, !, ?, +, *) and the opening paren
        let rest: &str = input.as_ref();
        let opens = rest
            .strip_prefix(['@', '!', '?', '+', '*'])
            .is_some_and(|after| after.starts_with('('));
        if !opens {
            return winnow::combinator::fail.parse_next(input);
        }
        // The tokenizer finds where the word holding the pattern ends: quotes, backquotes,
        // `$(...)` and `$'...'` in the pattern are read as in a full script. The rest of
        // that word is part of this piece of it.
        let options = super::parse_state::tokenizer_options();
        match crate::tokenizer::first_word_len(rest, &options) {
            Ok(len) if len >= 2 => Ok(input.next_slice(len)),
            // Unterminated: more input may complete it.
            _ => winnow::combinator::fail.parse_next(input),
        }
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

        loop {
            match next_char(input) {
                Ok('"') => break,
                Ok('\\') => {
                    let _ = next_char(input);
                }
                Err(_) => return fail.parse_next(input),
                Ok(_) => {}
            }
        }

        Ok(take_slice_from_checkpoints(input, &start))
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

// ---------------------------------------------------------------------------
// $ expansion skipping (shared by bracket expressions and balanced delimiters)
// ---------------------------------------------------------------------------

/// After `$` has been consumed, attempt to skip a `$`-expansion:
/// `$(...)`, `$((...))`, or `${...}`.
///
/// Returns `true` if an expansion was consumed. Returns `false` if
/// the next character doesn't start an expansion (stream is rewound to just
/// after the `$` — the caller is responsible for rewinding the `$` itself).
fn try_skip_dollar_expansion(input: &mut StrStream<'_>) -> bool {
    let checkpoint = input.checkpoint();
    match next_char(input) {
        Ok('(') => {
            if winnow::token::one_of::<_, _, ContextError>('(')
                .parse_next(input)
                .is_ok()
                && parse_balanced_delimiters("", Some('('), ')', 2)
                    .parse_next(input)
                    .is_ok()
            {
                return true;
            }
            // Not arithmetic: a command substitution, whose end the tokenizer finds.
            input.reset(&checkpoint);
            let _ = next_char(input);
            let rest: &str = input.as_ref();
            let options = super::parse_state::tokenizer_options();
            match crate::tokenizer::command_substitution_body(rest, &options) {
                Ok(body) => {
                    let _ = input.next_slice(body.len() + 1);
                    true
                }
                Err(_) => false,
            }
        }
        Ok('{') => parse_balanced_delimiters("", Some('{'), '}', 1)
            .parse_next(input)
            .is_ok(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Shared shell construct skipping (used by bracket expressions & balanced delimiters)
// ---------------------------------------------------------------------------

/// Skip the shell construct that `ch` starts, if any: a quoted string, an escape or a
/// `$`-expansion. `ch` has already been consumed; anything else is a plain character.
fn handle_shell_construct(ch: char, input: &mut StrStream<'_>) -> ModalResult<()> {
    match ch {
        '\\' => {
            let _ = next_char(input);
        }
        '\'' => {
            skip_single_quoted_content().parse_next(input)?;
        }
        '"' => {
            skip_double_quoted_content().parse_next(input)?;
        }
        '$' => {
            let checkpoint = input.checkpoint();
            if !try_skip_dollar_expansion(input) {
                input.reset(&checkpoint);
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Bracket expression consumer
// ---------------------------------------------------------------------------

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
/// - Single/double-quoted strings and `$`-expansions (their `]` chars are not treated as closing
///   the bracket expression)
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
        Ok(ch) => {
            handle_shell_construct(ch, input)?;
        }
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
            Ok(ch) => {
                handle_shell_construct(ch, input)?;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Parse a construct that ends at a balanced close delimiter: `${...}`, `$((...))`, `$[...]`,
/// `$"..."`, a bracket subscript or an extglob group. Quotes, escapes and nested
/// `$`-expansions inside it are skipped as a whole.
///
/// Returns the full slice, delimiters included.
///
/// # Parameters
/// - `prefix`: The opening delimiter(s) to match first (e.g., `${`)
/// - `open_char`: Character that increases depth (e.g., '(' or '{'), or None when only
///   `close_char` matters
/// - `close_char`: Character that decreases depth
/// - `initial_depth`: Starting depth (e.g., 1 for most, 2 for arithmetic `$((`)
pub(super) fn parse_balanced_delimiters<'a>(
    prefix: &'a str,
    open_char: Option<char>,
    close_char: char,
    initial_depth: usize,
) -> impl ModalParser<StrStream<'a>, &'a str, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();

        winnow::token::literal(prefix).parse_next(input)?;

        let mut depth = initial_depth;
        while depth > 0 {
            match next_char(input) {
                Ok(ch) if Some(ch) == open_char => depth += 1,
                Ok(ch) if ch == close_char => depth -= 1,
                Ok('[') => {
                    let checkpoint = input.checkpoint();
                    if try_consume_bracket_expression(input, close_char).is_err() {
                        input.reset(&checkpoint);
                    }
                }
                Ok(ch) => {
                    handle_shell_construct(ch, input)?;
                }
                Err(_) => return fail.parse_next(input),
            }
        }

        Ok(take_slice_from_checkpoints(input, &start))
    }
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
#[inline]
pub(super) fn newline<'a>() -> impl ModalParser<StrStream<'a>, char, ContextError> {
    |input: &mut StrStream<'a>| {
        let newline = '\n'.parse_next(input)?;
        // The line just ended may have carried here-document operators: their
        // bodies are the next lines, already read, and not commands.
        let here = input.current_token_start();
        if let Some(end) = super::parse_state::end_of_bodies(here) {
            let _ = input.next_slice(end - here);
        }
        Ok(newline)
    }
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
    repeat::<_, _, (), _, _>(0.., alt((blanks().void(), comment()))).void()
}

/// Parse one or more spaces and tabs.
#[inline]
pub(super) fn blanks<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    take_while(1.., [' ', '\t'])
}

/// Parse zero or more spaces and tabs.
#[inline]
pub(super) fn opt_blanks<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    take_while(0.., [' ', '\t'])
}

/// Parse required whitespace (at least one space or tab, optionally followed by comment)
#[inline]
pub(super) fn spaces1<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    (blanks(), winnow::combinator::opt(comment())).void()
}

/// Parse whitespace inside array literals `( ... )`.
/// Newlines are treated as whitespace separators, just like spaces and tabs.
/// Also handles comments and backslash-newline continuations.
#[inline]
pub(super) fn array_spaces<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    repeat::<_, _, (), _, _>(
        0..,
        winnow::combinator::alt((take_while(1.., [' ', '\t', '\n']).void(), comment())),
    )
    .void()
}

// ============================================================================
// Line breaks and separators
// ============================================================================

/// Parse linebreak (zero or more newlines, with optional comments before each newline)
/// Handles blank lines, comment-only lines, and lines with inline comments
#[inline]
pub(super) fn linebreak<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    repeat(0.., line_end())
}

/// Parse the end of a line: blanks, an optional comment and the newline.
fn line_end<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    (opt_blanks(), winnow::combinator::opt(comment()), newline()).void()
}

/// Parse newline list (one or more newlines, with optional comments before each newline)
/// Handles blank lines, comment-only lines, and lines with inline comments
#[inline]
pub(super) fn newline_list<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    repeat(1.., line_end())
}

/// Parse separator operator (';' or '&')
/// Must NOT be part of a longer operator like ';;', ';&', '&&', etc.
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
/// Corresponds to: peg.rs `separator()`
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
pub(super) fn name<'a>() -> impl ModalParser<StrStream<'a>, String, ContextError> {
    winnow::combinator::preceded(spaces(), super::words::bare_word())
        .verify(|s: &str| is_valid_name(s))
        .map(|s: &str| s.to_string())
}

/// Parse a function name.
pub(super) fn fname<'a>() -> impl ModalParser<StrStream<'a>, String, ContextError> {
    winnow::combinator::preceded(spaces(), super::words::bare_word_including_extglob())
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

// ============================================================================
// Comment-tracking variants of whitespace parsers
//
// These parallel parsers record comment byte ranges into `ctx.comments` as
// they consume whitespace.  They are used only at statement-boundary call
// sites in `program.rs` where comments are meaningful (between or trailing
// a complete command).  Inner call sites (inside `keyword`, `name`, etc.)
// continue using the zero-cost originals.
//
// Side-effect rule: never push into `ctx.comments` until the surrounding
// production has fully succeeded.  `comment_tracking` used to record the
// span as soon as `#...` matched; when it sat inside
// `(spaces, opt(comment), newline)` and `newline` then failed, winnow
// rewound the input but left the push in place — so a trailing
// no-newline comment was recorded three times (once per speculative
// attempt).  `comment_span` is the pure (no side-effect) building block;
// callers push only after the full match.
// ============================================================================

/// Parse a comment and return its byte range, without recording it.
fn comment_span<'a>() -> impl ModalParser<StrStream<'a>, std::ops::Range<usize>, ContextError> {
    move |input: &mut StrStream<'a>| {
        let start = input.current_token_start();
        ('#', take_while(0.., |c: char| c != '\n'))
            .void()
            .parse_next(input)?;
        let end = input.current_token_start();
        Ok(start..end)
    }
}

/// Parse a comment and record its byte range in `ctx.comments`.
///
/// Safe to use as a committed alternative (e.g. inside `alt` in
/// `spaces_tracking`): once the alternative is chosen the input is
/// consumed and no outer production will backtrack past it.
pub(super) fn comment_tracking<'a>(
    ctx: &'a super::types::ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let range = comment_span().parse_next(input)?;
        ctx.comments.borrow_mut().push(range);
        Ok(())
    }
}

/// Like `spaces()` but records inline comments into `ctx.comments`.
pub(super) fn spaces_tracking<'a>(
    ctx: &'a super::types::ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        repeat::<_, _, (), _, _>(0.., alt((blanks().void(), comment_tracking(ctx))))
            .void()
            .parse_next(input)
    }
}

/// Like `linebreak()` but records comment-only lines into `ctx.comments`.
pub(super) fn linebreak_tracking<'a>(
    ctx: &'a super::types::ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (), ContextError> + 'a {
    repeat(0.., line_end_tracking(ctx))
}

/// Like `newline_list()` but records comment-only lines into `ctx.comments`.
pub(super) fn newline_list_tracking<'a>(
    ctx: &'a super::types::ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (), ContextError> + 'a {
    repeat(1.., line_end_tracking(ctx))
}

/// Like `line_end()` but records the comment into `ctx.comments`.
///
/// The span is only pushed after the trailing newline is confirmed, so a
/// speculative match of a final no-newline comment does not leak a record.
fn line_end_tracking<'a>(
    ctx: &'a super::types::ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        opt_blanks().parse_next(input)?;
        let range = winnow::combinator::opt(comment_span()).parse_next(input)?;
        newline().parse_next(input)?;
        if let Some(range) = range {
            ctx.comments.borrow_mut().push(range);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::winnow_str::types::StrStream;

    fn parse_braced(input: &str) -> Result<&str, winnow::error::ErrMode<ContextError>> {
        let mut stream = StrStream::new(input);
        parse_balanced_delimiters("${", Some('{'), '}', 1).parse_next(&mut stream)
    }

    fn parse_cmd_sub(input: &str) -> Result<&str, winnow::error::ErrMode<ContextError>> {
        let mut stream = StrStream::new(input);
        super::super::words::command_substitution().parse_next(&mut stream)
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
