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

#[expect(clippy::cast_possible_truncation)]
fn expand_brace_expr_member(bem: word::BraceExpressionMember) -> Box<dyn Iterator<Item = String>> {
    match bem {
        word::BraceExpressionMember::NumberSequence {
            start,
            end,
            increment,
        } => {
            let increment = increment.unsigned_abs() as usize;

            if start <= end {
                Box::new((start..=end).step_by(increment).map(|n| n.to_string()))
            } else {
                Box::new(
                    (end..=start)
                        .step_by(increment)
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev(),
                )
            }
        }

        word::BraceExpressionMember::CharSequence {
            start,
            end,
            increment,
        } => {
            let increment = increment.unsigned_abs() as usize;

            if start <= end {
                Box::new((start..=end).step_by(increment).map(|c| c.to_string()))
            } else {
                Box::new(
                    (end..=start)
                        .step_by(increment)
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev(),
                )
            }
        }

        word::BraceExpressionMember::Child(elements) => {
            // Chain all element iterators together
            Box::new(generate_and_combine_brace_expansions(elements).into_iter())
        }
    }
}
