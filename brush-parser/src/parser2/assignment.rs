use std::{borrow::Cow, str::Utf8Error};

use crate::ast;

use winnow::{
    combinator::{alt, cut_err, delimited, opt, repeat, separated_pair, trace},
    prelude::*,
    stream::AsChar as _,
    token::{one_of, take_while},
    PResult,
};

use super::{
    custom_combinators::{expand_later, non_posix_extension},
    trivia::line_trailing,
    word, Input,
};

// assignment_word?
/// `assignment := name '+'? '=' ( array_value | scalar_value )`
pub fn assignment(i: &mut Input<'_>) -> PResult<(ast::Assignment, ast::Word)> {
    trace(
        "assignment",
        separated_pair(
            (assignment_name, opt("+").map(|append| append.is_some())),
            "=",
            cut_err(alt((non_posix_extension(array::array_value), scalar_value))),
        ),
    )
    .with_taken()
    .try_map::<_, _, Utf8Error>(|(((name, append), value), span)| {
        let assignment = ast::Assignment {
            name,
            value,
            append,
        };
        let word = ast::Word::new(std::str::from_utf8(span)?);
        Ok((assignment, word))
    })
    .parse_next(i)
}

/// NAME
/// In the shell command language, a word consisting solely of underscores, digits, and alphabetics
/// from the portable character set. The first character of a name is not a digit.
/// https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html#tag_03_216
/// `identifier := (_
/// | alpha) (_ | alphanum)` A word that is allowed in the assignment as a name
pub(crate) fn name<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
    trace(
        "identifier",
        (
            one_of(|c| (c as char).is_alpha() || c == b'_'),
            take_while(0.., |c| (c as char).is_alphanum() || c == b'_'),
        ),
    )
    .take()
    .try_map(std::str::from_utf8)
    .parse_next(i)
}

/// `name := identifier array-index? `
fn assignment_name(i: &mut Input<'_>) -> PResult<ast::AssignmentName> {
    trace(
        "name",
        (name, opt(array::array_index)).map(|(name, index)| {
            if let Some(index) = index {
                ast::AssignmentName::ArrayElementName(name.into(), index.into())
            } else {
                ast::AssignmentName::VariableName(name.into())
            }
        }),
    )
    .parse_next(i)
}

/// `scalar-value := string?`
pub fn scalar_value<'i>(i: &mut Input<'i>) -> PResult<ast::AssignmentValue> {
    trace(
        "scalar_value",
        // NOTE: maybe be empty: `a=`
        opt(alt((
            // a=[$(echo hello)]
            expand_later.map(|s| Cow::Borrowed(s)),
            word::word,
        )))
        .map(|s| s.unwrap_or_default()),
    )
    .map(|v| ast::AssignmentValue::Scalar(ast::Word { value: v.into() }))
    .parse_next(i)
}

mod array {
    use std::borrow::Cow;

    use winnow::combinator::cut_err;

    use crate::parser2::custom_combinators;

    use super::*;

    /// array-value := '(' (line-trailing* array-element line-trailing*)* ')'
    pub(super) fn array_value<'i>(i: &mut Input<'i>) -> PResult<ast::AssignmentValue> {
        trace(
            "array_value",
            delimited(
                "(",
                // TODO: I'm worried about line_trailing. it's api is not clear what it actually do
                repeat(0.., delimited(line_trailing, array_element, line_trailing)),
                cut_err(")"),
            )
            .map(|elements: Vec<_>| ast::AssignmentValue::Array(elements)),
        )
        .parse_next(i)
    }

    // NOTE: we cant possibly tell at this point what array is this: indexed (declare -a) or
    // associative (declare -A). The differences between them:
    // `name[subscript]=value` `subscript` in indexed array is an arithmetic expression, but in
    // associative it is an arbitrary string

    // manual https://www.gnu.org/software/bash/manual/html_node/Arrays.html states:
    // > associative arrays use arbitrary strings
    // > name[subscript]=value
    // > The subscript is treated as an **arithmetic** expression that must evaluate to a number. To
    // > explicitly declare an array, use

    /// array-index := '[' _* ']'
    pub fn array_index<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
        custom_combinators::take_inside(b'[', b']')
            // delimited(
            //     '[',
            //     custom_combinators::take_unil_unbalanced(b"[", b"]"),
            //     cut_err(']'),
            // )
            .try_map(std::str::from_utf8)
            .parse_next(i)
    }

    /// array-element := ( array-index '=' string? ) | string
    fn array_element<'i>(i: &mut Input<'i>) -> PResult<(Option<ast::Word>, ast::Word)> {
        alt((
            separated_pair(array_index, "=", opt(word::word)).try_map::<_, _, Utf8Error>(
                |(key, value)| {
                    Ok((
                        Some(ast::Word::new(key)),
                        ast::Word::from(String::from(value.unwrap_or(Cow::Borrowed("")))),
                    ))
                },
            ),
            word::word.map(|w| (None, ast::Word::from(String::from(w)))),
        ))
        .parse_next(i)
    }
}

#[cfg(test)]
mod tests {
    use crate::parser2::new_input;
    use crate::parser2::tests::input;
    use crate::parser2::tests::Result;

    use super::*;

    #[test]
    fn test_array() -> Result<()> {
        let i = input("a=( a  b c )");
        let expect = assignment.parse(i)?;
        dbg!(&expect);
        Ok(())
        // parse("a=");
    }

    #[test]
    fn test_subshell() -> Result<()> {
        let i = input("GPG_TTY=$(tty)");
        let expect = assignment.parse(i)?;
        dbg!(&expect);

        Ok(())
    }
}
