use winnow::error::ContextError;
use winnow::prelude::*;

use crate::ast;

use super::compound::{brace_group, do_group, for_clause, subshell};
use super::helpers::{keyword, linebreak, sequential_sep, spaces};
use super::position::PositionTracker;
use super::types::{ParseContext, StrStream};

// ============================================================================
// Arithmetic Expressions
// ============================================================================

/// Normalize an arithmetic expression string to match peg parser output.
/// The peg parser uses source position gaps to detect whitespace and inserts
/// exactly one space per gap. We replicate this by collapsing runs of
/// whitespace to a single space and trimming leading/trailing whitespace.
fn normalize_arithmetic_expr(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut had_space = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !result.is_empty() {
                had_space = true;
            }
        } else {
            if had_space {
                result.push(' ');
                had_space = false;
            }
            result.push(c);
        }
    }

    result
}

/// Parse an arithmetic expression up to the `))` or `;` that ends it at depth 0.
pub(super) fn arithmetic_expression<'a>()
-> impl ModalParser<StrStream<'a>, ast::UnexpandedArithmeticExpr, ContextError> {
    move |input: &mut StrStream<'a>| {
        let start = input.checkpoint();
        let mut paren_depth = 0i32;

        loop {
            if paren_depth == 0 {
                // `))` (spaces allowed between, as the tokenizer gives two tokens) or `;`
                let mut ahead = *input;
                spaces().parse_next(&mut ahead)?;
                let at_end: ModalResult<()> =
                    winnow::combinator::alt(((')', spaces(), ')').void(), ';'.void()))
                        .parse_next(&mut ahead);
                if at_end.is_ok() {
                    break;
                }
            }
            match winnow::token::any::<_, ContextError>.parse_next(input) {
                Ok('(') => paren_depth += 1,
                Ok(')') => paren_depth -= 1,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        let expr = super::helpers::take_slice_from_checkpoints(input, &start);
        Ok(ast::UnexpandedArithmeticExpr {
            value: normalize_arithmetic_expr(expr),
        })
    }
}

/// Parse arithmetic command (( expr ))
pub(super) fn arithmetic_command<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ArithmeticCommand, ContextError> + 'a {
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
pub(super) fn paren_compound<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompoundCommand, ContextError> + 'a {
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
// Arithmetic For Loops
// ============================================================================

/// Parse arithmetic for body (`do_group` or `brace_group`)
/// Corresponds to: peg.rs `arithmetic_for_body()`
/// Accepts: "; do", "\n do", or just " do" (spaces are consumed by keyword("do"))
fn arithmetic_for_body<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::DoGroupCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Consume optional linebreak (spaces, newlines, comments) before trying to match the body
        // This is needed because both do_group (via keyword) and brace_group
        // may have leading whitespace
        let _ = linebreak().parse_next(input);

        winnow::combinator::alt((
            // Try brace_group first (alternate syntax with braces)
            // Must come before do_group to avoid keyword("do") consuming spaces
            // Precede with spaces to handle leading whitespace
            winnow::combinator::preceded(spaces(), brace_group(ctx, tracker)).map(|bg| {
                ast::DoGroupCommand {
                    list: bg.list,
                    loc: bg.loc,
                }
            }),
            // Try sequential_sep followed by do_group (for "; do" or "\n do")
            winnow::combinator::preceded(sequential_sep(), do_group(ctx, tracker)),
            // Try do_group directly (for " do" - spaces consumed by keyword)
            do_group(ctx, tracker),
        ))
        .parse_next(input)
    }
}

/// Parse arithmetic for clause: for (( init; cond; update )) body
pub(super) fn arithmetic_for_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ArithmeticForClauseCommand, ContextError> + 'a {
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
pub(super) fn for_or_arithmetic_for<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompoundCommand, ContextError> + 'a {
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
