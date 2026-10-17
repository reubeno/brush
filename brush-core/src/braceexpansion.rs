use brush_parser::word;
use itertools::Itertools;

pub(crate) fn generate_and_combine_brace_expansions(
    pieces: Vec<brush_parser::word::BraceExpressionOrText>,
) -> impl IntoIterator<Item = String> {
    let expansions: Vec<Vec<String>> = pieces
        .into_iter()
        .map(|piece| expand_brace_expr_or_text(piece).collect())
        .collect();

    expansions
        .into_iter()
        .multi_cartesian_product()
        .map(|v| v.join(""))
}

fn expand_brace_expr_or_text(
    beot: word::BraceExpressionOrText,
) -> Box<dyn Iterator<Item = String>> {
    match beot {
        word::BraceExpressionOrText::Expr(members) => {
            // Chain all member iterators together
            Box::new(members.into_iter().flat_map(expand_brace_expr_member))
        }
        word::BraceExpressionOrText::Text(text) => Box::new(std::iter::once(text)),
    }
}

fn expand_brace_expr_member(bem: word::BraceExpressionMember) -> Box<dyn Iterator<Item = String>> {
    match bem {
        word::BraceExpressionMember::NumberSequence {
            start,
            end,
            increment,
            zero_padded_width,
        } => {
            let increment = increment.unsigned_abs().max(1);

            // A bound written with a leading zero asks for every member to be padded
            // out to the width of the longer bound, so `{01..10}` counts `01 02 ...`
            // and not `1 2 ...`. The sign takes one of those columns.
            let format = move |n: i64| match zero_padded_width {
                Some(width) => std::format!("{n:0width$}"),
                None => n.to_string(),
            };

            if start <= end {
                // Iterate from start up to end by incrementing, stopping rather than
                // overflowing when the next member would pass `i64::MAX`. Stepping in
                // `i64` also keeps an increment wider than `usize` from truncating on
                // 32-bit targets.
                Box::new(
                    std::iter::successors(Some(start), move |&n| {
                        n.checked_add_unsigned(increment)
                            .filter(|&next| next <= end)
                    })
                    .map(format),
                )
            } else {
                // Iterate from start down to end by decrementing, stopping rather than
                // overflowing when the next member would fall below `i64::MIN`.
                Box::new(
                    std::iter::successors(Some(start), move |&n| {
                        n.checked_sub_unsigned(increment)
                            .filter(|&next| next >= end)
                    })
                    .map(format),
                )
            }
        }

        word::BraceExpressionMember::CharSequence {
            start,
            end,
            increment,
        } => {
            let increment = increment.unsigned_abs().max(1);

            if start <= end {
                // An increment too large for a `usize` (on 32-bit targets) steps past
                // every character, so it saturates instead of truncating to zero.
                let increment = usize::try_from(increment).unwrap_or(usize::MAX);
                Box::new((start..=end).step_by(increment).map(|c| c.to_string()))
            } else {
                // Iterate from start down to end by decrementing. An increment too
                // large for a `u32` steps past every character, so it ends the sequence
                // after its first member instead of being truncated.
                let increment = u32::try_from(increment).unwrap_or(u32::MAX);
                Box::new(
                    std::iter::successors(Some(start), move |&c| {
                        let next = char::from_u32((c as u32).checked_sub(increment)?)?;
                        (next >= end).then_some(next)
                    })
                    .map(|c| c.to_string()),
                )
            }
        }

        word::BraceExpressionMember::Child(elements) => {
            // Chain all element iterators together
            Box::new(generate_and_combine_brace_expansions(elements).into_iter())
        }
    }
}
