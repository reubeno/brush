use winnow::combinator::repeat;
use winnow::prelude::*;

use crate::ast;

use super::helpers::{linebreak, spaces};
use super::pipelines::pipeline;
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};

// ============================================================================
// Tier 5: And/Or Lists
// ============================================================================

/// Parse and/or operator ('&&' or '||')
/// Corresponds to: winnow.rs `and_or_op()`
/// Returns true for And (&&), false for Or (||)
#[inline]
pub(super) fn and_or_op<'a>() -> impl Parser<StrStream<'a>, bool, PError> {
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
pub(super) fn and_or<'a>(
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
