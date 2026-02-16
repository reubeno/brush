use std::borrow::Cow;

use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::Offset;
use winnow::token::take_while;

use crate::ast;

use super::helpers::{
    extglob_pattern, parse_balanced_delimiters, peek_char, spaces1, tilde_expansion,
};
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};

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
pub(super) fn bare_word<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    take_while(1.., |c: char| {
        !matches!(
            c,
            ' ' | '\t' | '\n' | '\r' |  // Whitespace
            '|' | '&' | ';' | '<' | '>' | '(' | ')' |  // Operators (note: { } removed to allow brace expansion)
            '$' | '`' | '\'' | '"' | '\\' | // Quote/expansion starts
            '@' | '?' | '*' | '+' | '!' // Extglob prefixes — stop so word_part can dispatch
        )
    })
}

/// Parse a non-reserved word (for use as command names)
/// Reserved words cannot be used as command names in simple commands
pub(super) fn non_reserved_word<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Word, PError> + 'a {
    word_as_ast(ctx, tracker)
        .verify(|word: &ast::Word| !super::helpers::is_reserved_word(&word.value))
}

// ============================================================================
// Tier 9: Variable Expansions
// ============================================================================

/// Parse a simple variable reference: $VAR
/// Returns the expansion text including the $
pub(super) fn simple_variable<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    (
        '$',
        winnow::token::take_while(1.., |c: char| c.is_alphanumeric() || c == '_'),
    )
        .take()
}

/// Parse a braced variable reference: ${VAR}
/// Returns the expansion text including ${ }
pub(super) fn braced_variable<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("${", Some('{'), '}', 1)
}

/// Parse an arithmetic expansion: $((expr))
/// Returns the expansion text including $(( ))
pub(super) fn arithmetic_expansion<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("$((", Some('('), ')', 2)
}

/// Parse a command substitution: $(cmd)
/// Returns the expansion text including $( )
pub(super) fn command_substitution<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    // Need to be careful: $(( is arithmetic, $( is command substitution
    winnow::combinator::preceded(
        winnow::combinator::peek(winnow::combinator::not("$((")),
        parse_balanced_delimiters("$(", Some('('), ')', 1),
    )
}

/// Parse a backtick command substitution: `cmd`
/// Returns the expansion text including backticks
pub(super) fn backtick_substitution<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("`", None, '`', 1)
}

/// Parse special parameter: $0, $1, $?, $@, etc.
/// Returns the expansion text including the $
pub(super) fn special_parameter<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
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
pub(super) fn single_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
    ('\'', take_while(0.., |c: char| c != '\''), '\'')
        .take()
        .map(|s: &str| s.to_string())
}

/// Parse an ANSI-C quoted string: $'text'.
/// Returns the full string including the $'...' syntax (e.g., `$'text'`).
/// In ANSI-C quotes, backslash escapes are processed specially.
pub(super) fn ansi_c_quoted_string<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("$'", None, '\'', 1)
}

/// Parse a gettext-style double-quoted string: $"text".
/// Returns the full string including the $"..." syntax (e.g., `$"text"`).
/// This is used for localization in bash.
pub(super) fn gettext_double_quoted_string<'a>() -> impl Parser<StrStream<'a>, &'a str, PError> {
    parse_balanced_delimiters("$\"", None, '"', 1)
}

/// Parse a double-quoted string: "text".
///
/// Returns the full string including quotes (e.g., `"text"`).
/// Handles backslash escape sequences and `$(...)` command substitutions
/// (which may span multiple lines for heredocs) inside the string.
pub(super) fn double_quoted_string<'a>() -> impl Parser<StrStream<'a>, String, PError> {
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

/// Parse an escape sequence: `\c`, or `\<extglob_prefix>(…)` when extglob is
/// enabled.
///
/// The tokenizer enters extglob mode whenever the last consumed character can
/// start an extglob and the next character is `(`, even when the prefix was
/// escaped.  We mirror that here: `\@(pattern)` is returned as a single slice
/// `\@(pattern)` so it stays in the same word.
///
/// Returns the full consumed slice including the leading backslash.
pub(super) fn escape_sequence<'a>(
    extglob_enabled: bool,
) -> impl Parser<StrStream<'a>, &'a str, PError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();
        '\\'.parse_next(input)?;
        let c = winnow::token::any::<_, PError>.parse_next(input)?;

        // If the escaped char is an extglob prefix and '(' follows, consume
        // the balanced parens so the whole construct stays in this word.
        if extglob_enabled && matches!(c, '@' | '?' | '*' | '+' | '!') {
            let _ = winnow::combinator::opt(parse_balanced_delimiters("(", Some('('), ')', 1))
                .parse_next(input)?;
        }

        let end = input.checkpoint();
        let len = end.offset_from(&start);
        input.reset(&start);
        winnow::token::take(len).parse_next(input)
    }
}

/// Parse a word part (bare text, single quote, double quote, escape, or expansion)
/// Returns the string value of the part
/// The `last_char` parameter helps detect tilde-after-colon
pub(super) fn word_part<'a>(
    ctx: &'a ParseContext<'a>,
    last_char: Option<char>,
) -> impl Parser<StrStream<'a>, Cow<'a, str>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Fast path: dispatch on first character
        let ch = peek_char().parse_next(input)?;

        match ch {
            '\'' => single_quoted_string().map(Cow::Owned).parse_next(input),
            '"' => double_quoted_string().map(Cow::Owned).parse_next(input),
            '$' => {
                winnow::combinator::alt((
                    ansi_c_quoted_string(),         // $'
                    gettext_double_quoted_string(), // $" before $(
                    arithmetic_expansion(),         // $(( before $(
                    command_substitution(),         // $(
                    braced_variable(),              // ${ before $
                    special_parameter(),            // $1, $?, etc. before simple $VAR
                    simple_variable(),              // $VAR
                ))
                .map(Cow::Borrowed)
                .parse_next(input)
            }
            '`' => backtick_substitution().map(Cow::Borrowed).parse_next(input),
            '\\' => escape_sequence(ctx.options.enable_extended_globbing)
                .map(Cow::Borrowed)
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
                    // Not an extglob — consume just the prefix char as a literal.
                    // bare_word() stops at these chars, so the next word_part
                    // iteration will continue with whatever follows.
                    winnow::token::take(1usize)
                        .map(Cow::Borrowed)
                        .parse_next(input)
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
pub(super) fn word_as_ast<'a>(
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
pub(super) fn wordlist<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::Word>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::separated(1.., word_as_ast(ctx, tracker), spaces1()).parse_next(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::winnow_str::types::StrStream;

    #[test]
    fn test_ansi_c_quoted_string_simple() {
        let input = StrStream::new("$'hello'");
        let result = super::ansi_c_quoted_string().parse_next(&mut input.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "$'hello'");
    }

    #[test]
    fn test_ansi_c_quoted_string_with_escape() {
        let input = StrStream::new("$'\\n'");
        let result = super::ansi_c_quoted_string().parse_next(&mut input.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "$'\\n'");
    }
}
