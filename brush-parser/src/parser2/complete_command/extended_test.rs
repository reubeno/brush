use winnow::combinator::{cut_err, delimited, empty, peek, separated_pair, terminated};
use winnow::token::{any, one_of, take_till, take_until};

use crate::parser2::trivia::space;
use crate::parser2::word;

use self::custom_combinators::take_inside;

use super::*;

// TODO: https://github.com/oils-for-unix/oils/issues/3

pub fn extended_test_command(i: &mut Input<'_>) -> PResult<ast::ExtendedTestExpr> {
    delimited(
        "[[",
        terminated(
            trim_whitespace(0.., extended_test_expression, 0..),
            line_trailing,
        ),
        "]]",
    )
    .parse_next(i)
}

fn extended_test_expression(i: &mut Input<'_>) -> PResult<ast::ExtendedTestExpr> {
    trace(
        "extended_test_expression",
        custom_combinators::precedence(
            // prefix operators
            alt((custom_combinators::unary_op(
                1,
                trim_whitespace(0.., "!", 0..),
            ),)),
            // postfix operators
            fail,
            // binary operators
            alt((
                custom_combinators::binary_op(
                    2,
                    custom_combinators::Assoc::Left,
                    delimited(
                        repeat(0.., line_trailing).map(|()| ()),
                        trim_whitespace(0.., "||", 0..),
                        repeat(0.., line_trailing).map(|()| ()),
                    ),
                ),
                custom_combinators::binary_op(
                    2,
                    custom_combinators::Assoc::Left,
                    delimited(
                        repeat(0.., line_trailing).map(|()| ()),
                        trim_whitespace(0.., "&&", 0..),
                        repeat(0.., line_trailing).map(|()| ()),
                    ),
                ),
            )),
            // operands
            alt((
                delimited(
                    "(",
                    trim_whitespace(0.., extended_test_expression, 0..),
                    ")",
                ), //subexpression handled via recursion
                binary_test,
                separated_pair(conditional_unary_operator, space(1..), word::word).map(|(p, w)| {
                    ast::ExtendedTestExpr::UnaryTest(p, ast::Word::from(String::from(w)))
                }),
                word::word.map(|w| {
                    ast::ExtendedTestExpr::UnaryTest(
                        ast::UnaryPredicate::StringHasNonZeroLength,
                        ast::Word::from(String::from(w)),
                    )
                }),
            )),
            |op: custom_combinators::Operation<&[u8], &[u8], &[u8], _>| {
                //evaluating the expression step by step
                use self::custom_combinators::Operation::*;
                match op {
                    Prefix(b"!", o) => Ok(ast::ExtendedTestExpr::Not(Box::from(o))),
                    Binary(lhs, b"||", rhs) => {
                        Ok(ast::ExtendedTestExpr::Or(Box::from(lhs), Box::from(rhs)))
                    }
                    Binary(lhs, b"&&", rhs) => {
                        Ok(ast::ExtendedTestExpr::And(Box::from(lhs), Box::from(rhs)))
                    }
                    _ => Err("Invalid combination"),
                }
            },
        ),
    )
    .parse_next(i)
}

fn binary_test(i: &mut Input<'_>) -> PResult<ast::ExtendedTestExpr> {
    trace(
        "binary_test",
        alt((
            (
                word::word,
                trim_whitespace(0.., binary_predicate, 0..),
                word::word,
            )
                .map(|(l, p, r)| {
                    ast::ExtendedTestExpr::BinaryTest(
                        p,
                        ast::Word::from(l.to_string()),
                        ast::Word::from(r.to_string()),
                    )
                }),
            binary_test_regex,
        )),
    )
    .parse_next(i)
}
fn binary_predicate(i: &mut Input<'_>) -> PResult<ast::BinaryPredicate> {
    let v = |v| empty.value(v);
    trace(
            "binary_predicate_basic",
            terminated(alt((
                one_of((b"=", b"==")).value(ast::BinaryPredicate::StringExactlyMatchesPattern),
                "!=".value(ast::BinaryPredicate::StringDoesNotExactlyMatchPattern),
                "<".value(ast::BinaryPredicate::LeftSortsBeforeRight),
                ">".value(ast::BinaryPredicate::LeftSortsAfterRight),
                preceded(
                "-",
                dispatch! {take::<_, Input<'_>, _>(2usize);
                    b"ef" => v(ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers),
                    b"eq" => v(ast::BinaryPredicate::ArithmeticEqualTo),
                    b"ge" => v(ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo),
                    b"gt" => v(ast::BinaryPredicate::ArithmeticGreaterThan),
                    b"le" => v(ast::BinaryPredicate::ArithmeticLessThanOrEqualTo),
                    b"lt" => v(ast::BinaryPredicate::ArithmeticLessThan),
                    b"ne" => v(ast::BinaryPredicate::ArithmeticNotEqualTo),
                    b"nt" => v(ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot),
                    b"ot" => v(ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes),
                    _ => fail,
                },
            ),

            )), space(1..)),
        )
        .parse_next(i)
}

fn binary_test_regex(i: &mut Input<'_>) -> PResult<ast::ExtendedTestExpr> {
    trace(
        "binary_test_regex",
        (word::word, trim_whitespace(0.., "=~", 0..), regex_word).map(|(l, _, r)| {
            if r.starts_with(['\'', '\"']) {
                // TODO: Confirm it ends with that too?
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringContainsSubstring,
                    ast::Word::from(l.into_owned()),
                    ast::Word::new(r),
                )
            } else {
                ast::ExtendedTestExpr::BinaryTest(
                    ast::BinaryPredicate::StringMatchesRegex,
                    ast::Word::from(l.into_owned()),
                    ast::Word::new(r),
                )
            }
        }),
    )
    .parse_next(i)
}

fn conditional_unary_operator(i: &mut Input<'_>) -> PResult<ast::UnaryPredicate> {
    let v = |v| empty.value(v);
    trace(
        "extended_unary_expression",
        preceded(
            "-",
            dispatch! {any;
                b'a' => v(ast::UnaryPredicate::FileExists),
                b'b' => v(ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile),
                b'c' => v(ast::UnaryPredicate::FileExistsAndIsCharSpecialFile),
                b'd' => v(ast::UnaryPredicate::FileExistsAndIsDir),
                b'e' => v(ast::UnaryPredicate::FileExists),
                b'f' => v(ast::UnaryPredicate::FileExistsAndIsRegularFile),
                b'g' => v(ast::UnaryPredicate::FileExistsAndIsSetgid),
                b'h' => v(ast::UnaryPredicate::FileExistsAndIsSymlink),
                b'k' => v(ast::UnaryPredicate::FileExistsAndHasStickyBit),
                b'n' => v(ast::UnaryPredicate::StringHasNonZeroLength),
                b'o' => v(ast::UnaryPredicate::ShellOptionEnabled),
                b'p' => v(ast::UnaryPredicate::FileExistsAndIsFifo),
                b'r' => v(ast::UnaryPredicate::FileExistsAndIsReadable),
                b's' => v(ast::UnaryPredicate::FileExistsAndIsNotZeroLength),
                b't' => v(ast::UnaryPredicate::FdIsOpenTerminal),
                b'u' => v(ast::UnaryPredicate::FileExistsAndIsSetuid),
                b'v' => v(ast::UnaryPredicate::ShellVariableIsSetAndAssigned),
                b'w' => v(ast::UnaryPredicate::FileExistsAndIsWritable),
                b'x' => v(ast::UnaryPredicate::FileExistsAndIsExecutable),
                b'z' => v(ast::UnaryPredicate::StringHasZeroLength),
                b'G' => v(ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId),
                b'L' => v(ast::UnaryPredicate::FileExistsAndIsSymlink),
                b'N' => v(ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead),
                b'O' => v(ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId),
                b'R' => v(ast::UnaryPredicate::ShellVariableIsSetAndNameRef),
                b'S' => v(ast::UnaryPredicate::FileExistsAndIsSocket),
                _ => fail

            },
        ),
    )
    .parse_next(i)
}

fn regex_word<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
    const REGEX_STOP: &[u8] = &[b' ', b'"', b'\'', b'\r', b'\n', b'\t'];
    repeat(
        1..,
        dispatch! {peek(any);
            b'(' => take_inside(b'(', b')'),
            b'"' => delimited("\"", take_until(0.., "\""), cut_err("\"")),
            b'\'' => delimited("'", take_until(0.., "'"),  cut_err("'")),
            _ => take_till(1.., REGEX_STOP)
        },
    )
    .with_taken()
    .try_map(|((), s)| std::str::from_utf8(s))
    .parse_next(i)
}

#[cfg(test)]
mod tests {
    use crate::parser2::new_input;
    use crate::ParserOptions;

    use super::*;
    #[test]
    fn test_parse_regex_word() {
        let input = r#"[[ " sss " =~ ( ss()s ) ]]"#;
        let r = extended_test_command.parse(new_input(ParserOptions::default(), input));
        dbg!(r);

        // let input = r#"[[ a =~ ^[0-9]{8}$" "]] ]]"#;
    }
}
