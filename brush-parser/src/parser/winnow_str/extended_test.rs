use winnow::combinator::{fail, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::take_while;

use crate::ast;

use super::helpers::{comment, peek_char, spaces};
use super::position::PositionTracker;
use super::types::{ParseContext, StrStream};
use super::words::{dollar_expansion, double_quoted_string, single_quoted_string};

// ============================================================================
// Extended Test Expressions [[ ]]
// ============================================================================

/// Parse whitespace inside extended test [[ ]] expressions.
/// Unlike `spaces()`, this also skips newlines, because bash allows multi-line [[ ]]
/// expressions.
#[inline]
fn ext_test_spaces<'a>() -> impl ModalParser<StrStream<'a>, (), ContextError> {
    repeat::<_, _, (), _, _>(
        0..,
        winnow::combinator::alt((
            take_while(1.., |c: char| c == ' ' || c == '\t' || c == '\n').void(),
            comment(), // # comments
        )),
    )
    .void()
}

/// Parse a unary test operator (-f, -z, -n, etc.)
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

/// Parse a single-quoted string segment and append to word
fn ext_test_parse_single_quoted(input: &mut StrStream<'_>, word: &mut String) -> ModalResult<()> {
    word.push_str(single_quoted_string().parse_next(input)?);
    Ok(())
}

/// Parse a double-quoted string segment and append to word
fn ext_test_parse_double_quoted(input: &mut StrStream<'_>, word: &mut String) -> ModalResult<()> {
    word.push_str(double_quoted_string().parse_next(input)?);
    Ok(())
}

/// Parse a backslash escape sequence and append to word
fn ext_test_parse_backslash_escape(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> ModalResult<()> {
    winnow::token::any.parse_next(input)?;
    word.push('\\');
    if let Ok(c) = winnow::token::any::<_, ContextError>.parse_next(input) {
        word.push(c);
    }
    Ok(())
}

/// Parse a `$`-expansion and append it to word; a `$` that starts none is literal.
fn ext_test_parse_dollar_expansion(
    input: &mut StrStream<'_>,
    word: &mut String,
) -> ModalResult<()> {
    if let Some(expansion) = winnow::combinator::opt(dollar_expansion()).parse_next(input)? {
        word.push_str(expansion);
    } else {
        winnow::token::any.parse_next(input)?;
        word.push('$');
    }
    Ok(())
}

/// Parse special characters (&, |, !) and handle accordingly
fn ext_test_parse_special_char(
    input: &mut StrStream<'_>,
    word: &mut String,
    ch: char,
) -> ModalResult<bool> {
    // Returns Ok(true) if parsing should continue, Ok(false) if should stop
    match ch {
        '&' | '|' => {
            // A doubled operator (`&&`, `||`) ends the word; a single one is part of it.
            let checkpoint = input.checkpoint();
            winnow::token::any.parse_next(input)?;
            if peek_char().parse_next(input).ok() == Some(ch) {
                input.reset(&checkpoint);
                Ok(false)
            } else {
                word.push(ch);
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
) -> impl ModalParser<StrStream<'a>, ast::Word, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);
        let mut word = String::new();

        while let Ok(ch) = peek_char().parse_next(input) {
            match ch {
                '\'' => {
                    ext_test_parse_single_quoted(input, &mut word)?;
                }
                '"' => {
                    ext_test_parse_double_quoted(input, &mut word)?;
                }
                ' ' | '\t' | '\n' => break,
                '\\' => {
                    ext_test_parse_backslash_escape(input, &mut word)?;
                }
                '$' => {
                    ext_test_parse_dollar_expansion(input, &mut word)?;
                }
                '(' | ')' => break,
                '&' | '|' | '!' => {
                    if !ext_test_parse_special_char(input, &mut word, ch)? {
                        break;
                    }
                }
                _ => {
                    word.push(ch);
                    winnow::token::any.parse_next(input)?;
                }
            }
        }

        if word.is_empty() {
            fail.parse_next(input)
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

/// After a quoted string in a regex word, parse the word characters joined to it: up to a
/// blank, an operator, a parenthesis, the next quote, or a `]` that closes a bracket
/// expression.
fn regex_word_after_quote(
    input: &mut StrStream<'_>,
    result: &mut String,
    bracket_depth: usize,
) -> ModalResult<()> {
    while let Ok(ch) = peek_char().parse_next(input) {
        match ch {
            ' ' | '\t' | '\n' | '&' | '|' | '(' | ')' | '\'' | '"' => break,
            ']' if bracket_depth > 0 => break,
            '\\' => {
                winnow::token::any.parse_next(input)?;
                if let Ok(c) = winnow::token::any::<_, ContextError>.parse_next(input) {
                    result.push('\\');
                    result.push(c);
                }
            }
            _ => {
                result.push(ch);
                winnow::token::any.parse_next(input)?;
            }
        }
    }
    Ok(())
}

/// Parse a regex word in extended test context (allows | ( ) [ ] in the pattern)
#[allow(
    clippy::too_many_lines,
    reason = "one arm per character class of a regex word"
)]
fn ext_test_regex_word<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Word, ContextError> + 'a {
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
                && winnow::combinator::opt(winnow::combinator::alt((
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
                    '\'' => {
                        // Single-quoted string - parse it and continue for more word chars
                        result.push_str(single_quoted_string().parse_next(input)?);
                        regex_word_after_quote(input, &mut result, bracket_depth)?;
                    }
                    '"' => {
                        // Double-quoted string - parse it and continue for more word chars
                        result.push_str(double_quoted_string().parse_next(input)?);
                        regex_word_after_quote(input, &mut result, bracket_depth)?;
                    }
                    '$' => {
                        // Dollar expansion
                        ext_test_parse_dollar_expansion(input, &mut result)?;
                    }
                    '[' => {
                        // Start of bracket expression in regex
                        // Only increment if not already inside a bracket expression
                        // (in ERE, bracket expressions don't nest - [ inside [...] is literal)
                        if bracket_depth == 0 {
                            bracket_depth += 1;
                        }
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    ']' => {
                        // End of bracket expression in regex
                        bracket_depth = bracket_depth.saturating_sub(1);
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
                        paren_depth = paren_depth.saturating_sub(1);
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
                    '\\' => {
                        // Handle backslash escape directly to avoid ext_test_word
                        // consuming too much (e.g., the closing ] of a bracket expression)
                        winnow::token::any.parse_next(input)?; // consume \
                        let escaped: ModalResult<char> = winnow::token::any.parse_next(input);
                        result.push('\\');
                        if let Ok(c) = escaped {
                            result.push(c);
                        }
                    }
                    ' ' | '\t' | '\n'
                        if !result.is_empty() && bracket_depth == 0 && paren_depth == 0 =>
                    {
                        // Stop on whitespace after we've collected something
                        // But not if we're inside a bracket expression or parentheses
                        break;
                    }
                    ' ' | '\t' | '\n' if bracket_depth > 0 || paren_depth > 0 => {
                        // Whitespace inside bracket expression or parentheses is part of the regex
                        result.push(ch);
                        winnow::token::any.parse_next(input)?;
                    }
                    _ => {
                        // Regular word characters, taken verbatim: no separator is ever
                        // added, so `${CTARGET}-${PV}` stays one word.
                        let word = take_while(1.., |c: char| {
                            !matches!(
                                c,
                                ' ' | '\t'
                                    | '\n'
                                    | '&'
                                    | '|'
                                    | '('
                                    | ')'
                                    | '['
                                    | ']'
                                    | '\''
                                    | '"'
                                    | '\\'
                            )
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
            fail.parse_next(input)
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

/// Parse primary extended test expression (parentheses, binary/unary tests, or word).
///
/// Returns whether a newline may follow the parsed term before the next
/// separator (`]]`, `)`, `&&`, or `||`). This mirrors the peg grammar, which
/// places `linebreak()` after every primary form *except* a bare-word string
/// test -- `[[ x \n ]]` is a syntax error in bash, but `[[ -n x \n ]]` is not.
fn ext_test_primary<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::ExtendedTestExpr, bool), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        ext_test_spaces().parse_next(input)?;

        // Try parenthesized expression
        if winnow::combinator::opt('(').parse_next(input)?.is_some() {
            ext_test_spaces().parse_next(input)?;
            let (expr, inner_trailing_ok) = ext_test_or_expr(tracker).parse_next(input)?;
            if inner_trailing_ok {
                ext_test_spaces().parse_next(input)?;
            } else {
                spaces().parse_next(input)?;
            }
            ')'.parse_next(input)?;
            return Ok((ast::ExtendedTestExpr::Parenthesized(Box::new(expr)), true));
        }

        // Try unary test (operator + operand)
        let checkpoint = input.checkpoint();
        if let Ok(op_word) = ext_test_word(tracker).parse_next(input) {
            if let Some(unary_pred) = parse_unary_operator(&op_word.value) {
                ext_test_spaces().parse_next(input)?;
                let operand = ext_test_word(tracker).parse_next(input)?;
                return Ok((ast::ExtendedTestExpr::UnaryTest(unary_pred, operand), true));
            }
        }
        input.reset(&checkpoint);

        // Try binary test (operand + operator + operand)
        let left_word = ext_test_word(tracker).parse_next(input)?;
        // Checkpoint before consuming inter-token whitespace: if this doesn't
        // turn out to be a binary operator, we fall back to treating
        // `left_word` as a bare-word test, which must not have swallowed any
        // newline that belongs to the trailing separator check instead.
        let checkpoint2 = input.checkpoint();
        ext_test_spaces().parse_next(input)?;

        // Check for binary operator
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

                return Ok((
                    ast::ExtendedTestExpr::BinaryTest(binary_pred, left_word, right_word),
                    true,
                ));
            }
        }
        input.reset(&checkpoint2);

        // Fallback: single word tests for non-zero length. No trailing
        // newline is allowed here (see the function doc comment above).
        Ok((
            ast::ExtendedTestExpr::UnaryTest(
                ast::UnaryPredicate::StringHasNonZeroLength,
                left_word,
            ),
            false,
        ))
    }
}

/// Parse NOT expression (right-associative)
fn ext_test_not_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::ExtendedTestExpr, bool), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        ext_test_spaces().parse_next(input)?;

        // Check for NOT operator
        let checkpoint = input.checkpoint();
        if winnow::combinator::opt('!').parse_next(input)?.is_some() {
            // Make sure it's not != operator
            if peek_char().parse_next(input).ok() == Some('=') {
                input.reset(&checkpoint);
                return ext_test_primary(tracker).parse_next(input);
            }

            // Parse NOT recursively (right-associative). The trailing-newline
            // eligibility is inherited from the wrapped expression.
            let (expr, trailing_ok) = ext_test_not_expr(tracker).parse_next(input)?;
            return Ok((ast::ExtendedTestExpr::Not(Box::new(expr)), trailing_ok));
        }

        ext_test_primary(tracker).parse_next(input)
    }
}

/// Parse AND expression (left-associative)
fn ext_test_and_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::ExtendedTestExpr, bool), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (mut left, mut trailing_ok) = ext_test_not_expr(tracker).parse_next(input)?;

        loop {
            // A newline may only precede `&&` if the left operand just parsed
            // ended in a complete term, not a bare-word string test.
            if trailing_ok {
                ext_test_spaces().parse_next(input)?;
            } else {
                spaces().parse_next(input)?;
            }
            let checkpoint = input.checkpoint();

            // Check for && operator
            if winnow::combinator::opt("&&").parse_next(input)?.is_some() {
                let (right, right_trailing_ok) = ext_test_not_expr(tracker).parse_next(input)?;
                left = ast::ExtendedTestExpr::And(Box::new(left), Box::new(right));
                trailing_ok = right_trailing_ok;
            } else {
                input.reset(&checkpoint);
                break;
            }
        }

        Ok((left, trailing_ok))
    }
}

/// Parse OR expression (left-associative, lowest precedence)
fn ext_test_or_expr<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::ExtendedTestExpr, bool), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (mut left, mut trailing_ok) = ext_test_and_expr(tracker).parse_next(input)?;

        loop {
            // A newline may only precede `||` if the left operand just parsed
            // ended in a complete term, not a bare-word string test.
            if trailing_ok {
                ext_test_spaces().parse_next(input)?;
            } else {
                spaces().parse_next(input)?;
            }
            let checkpoint = input.checkpoint();

            // Check for || operator
            if winnow::combinator::opt("||").parse_next(input)?.is_some() {
                let (right, right_trailing_ok) = ext_test_and_expr(tracker).parse_next(input)?;
                left = ast::ExtendedTestExpr::Or(Box::new(left), Box::new(right));
                trailing_ok = right_trailing_ok;
            } else {
                input.reset(&checkpoint);
                break;
            }
        }

        Ok((left, trailing_ok))
    }
}

/// Parse extended test command: [[ expression ]]
pub(super) fn extended_test_command<'a>(
    _ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ExtendedTestExprCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse [[
        '['.parse_next(input)?;
        '['.parse_next(input)?;

        // Once we've seen [[, we're committed - any error should not backtrack
        // This ensures parse errors are reported at the actual error location, not "end of input"
        ext_test_spaces().parse_next(input)?;
        let (expr, trailing_ok) = ext_test_or_expr(tracker)
            .parse_next(input)
            .map_err(|e| e.cut())?;
        // A newline may only precede `]]` if the expression just parsed ended
        // in a complete term, not a bare-word string test.
        if trailing_ok {
            ext_test_spaces().parse_next(input).map_err(|e| e.cut())?;
        } else {
            spaces().parse_next(input).map_err(|e| e.cut())?;
        }
        ']'.parse_next(input)
            .map_err(|e: winnow::error::ErrMode<ContextError>| e.cut())?;
        ']'.parse_next(input)
            .map_err(|e: winnow::error::ErrMode<ContextError>| e.cut())?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::ExtendedTestExprCommand { expr, loc })
    }
}
