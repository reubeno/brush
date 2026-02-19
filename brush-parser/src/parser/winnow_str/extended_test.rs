use winnow::combinator::repeat;
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::take_while;

use crate::ast;

use super::helpers::{
    comment, peek_char, skip_double_quoted_content, skip_single_quoted_content, spaces,
};
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};
use super::words::double_quoted_string;

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
        "-a" | "-e" => Some(UnaryPredicate::FileExists),
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
                let escaped: Result<char, PError> = winnow::token::any.parse_next(input);
                if let Ok(c) = escaped {
                    out.push(c);
                }
            }
            '\'' => {
                let content = skip_single_quoted_content().parse_next(input)?;
                out.push_str(content);
            }
            '"' => {
                let content = skip_double_quoted_content().parse_next(input)?;
                out.push_str(content);
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
        let mut bracket_depth: usize = 0;
        let mut paren_depth: usize = 0;

        loop {
            // Skip whitespace between parts
            if result.is_empty() {
                spaces().parse_next(input)?;
            }

            let checkpoint = input.checkpoint();

            // Check if we hit a stop condition (&&, ||, ]], or end)
            // Only check for ]] when not inside a bracket expression or parentheses
            if bracket_depth == 0
                && paren_depth == 0
                && winnow::combinator::opt::<_, _, PError, _>(winnow::combinator::alt((
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
                    '[' => {
                        // Start of bracket expression in regex
                        bracket_depth += 1;
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    ']' => {
                        // End of bracket expression in regex
                        if bracket_depth > 0 {
                            bracket_depth -= 1;
                        }
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    '(' => {
                        // Start of group or extglob pattern
                        paren_depth += 1;
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    ')' => {
                        // End of group or extglob pattern
                        if paren_depth > 0 {
                            paren_depth -= 1;
                        }
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
                let is_pattern_op = matches!(
                    binary_pred,
                    ast::BinaryPredicate::StringExactlyMatchesPattern
                        | ast::BinaryPredicate::StringDoesNotExactlyMatchPattern
                );
                ext_test_spaces().parse_next(input)?;

                // For =~ operator, use regex word parser that allows | ( )
                // For == and != operators, also use regex word parser to support extglob patterns
                let right_word = if is_regex_op || is_pattern_op {
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
pub(super) fn extended_test_command<'a>(
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
