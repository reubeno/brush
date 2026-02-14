use winnow::prelude::*;

use crate::ast;

use super::compound::{brace_group, do_group, for_clause, subshell};
use super::helpers::{keyword, sequential_sep, spaces};
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};

// ============================================================================
// Tier 15: Arithmetic Expressions
// ============================================================================

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

pub(super) fn arithmetic_expression<'a>() -> impl Parser<StrStream<'a>, ast::UnexpandedArithmeticExpr, PError>
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
pub(super) fn arithmetic_command<'a>(
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
pub(super) fn paren_compound<'a>(
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
pub(super) fn arithmetic_for_clause<'a>(
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
pub(super) fn for_or_arithmetic_for<'a>(
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
