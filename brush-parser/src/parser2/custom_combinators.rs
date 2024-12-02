use winnow::{
    ascii::take_escaped,
    combinator::{cut_err, delimited, fail, opt, peek, trace},
    dispatch,
    error::{ContextError, ErrMode, ErrorKind, ParserError},
    stream::{AsBytes, Offset as _, Stream},
    token::{any, literal, one_of, take, take_till, take_until},
    PResult, Parser,
};

use crate::parser2::trivia::ESCAPE;

use super::Input;

pub(crate) fn non_posix_extension<'i, 's, ParseNext, Output>(
    mut parser: ParseNext,
) -> impl Parser<Input<'i>, Output, ContextError>
where
    ParseNext: Parser<Input<'i>, Output, ContextError>,
{
    trace("non_posix_extension", move |i: &mut Input<'i>| {
        if !i.state.sh_mode {
            parser.parse_next(i)
        } else {
            // TODO: errors
            Err(ErrMode::from_error_kind(i, ErrorKind::Verify))
        }
    })
}

// Temporary special case for subshell that goes onto the cmd_name or cmd_suffix
// in the original parser
pub(crate) fn expand_later<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
    trace(
        "expand_later",
        dispatch! {any;
            b'$' => take_inside(b'(', b')').void(),
            b'`' => take_till(0.., b'`').void(),
            _ => fail,
        }
        .take()
        .try_map(std::str::from_utf8),
    )
    .parse_next(i)
}

pub(crate) fn take_inside<'i, 's>(
    opening: u8,
    closing: u8,
) -> impl Parser<Input<'i>, <Input<'i> as Stream>::Slice, ContextError> {
    move |i: &mut Input<'i>| {
        trace("take_inside", move |i: &mut Input<'i>| {
            trace("opening", opening).parse_next(i)?;
            let start = i.checkpoint();
            cut_err(
                (move |i: &mut Input<'i>| {
                    let mut opened: u32 = 1;
                    while i.eof_offset() > 0
                        && opened != 0
                        // N.B it does not consumes characters from the second param ->
                        && opt(take_till(0.., (opening, closing, ESCAPE))).parse_next(i)?.is_some()
                    {
                        // -> consume this characters: (opening, closing or escape)
                        match i.next_token().unwrap() {
                            ESCAPE => {
                                opt(any).parse_next(i)?;
                            }
                            c if c == opening => {
                                opened += 1;
                            }
                            c if c == closing => {
                                opened -= 1;
                            }
                            // Can not happen.
                            _ => unreachable!(),
                        };
                        // special case. but it is meaningless to use this function
                        // with the same brackets (e.g `"`) because the next one
                        // is always the closing
                        // `[abc]`d`fg`
                        if opening == closing {
                            opened %= 2;
                        }
                    }

                    Ok(opened)
                })
                // TODO: error unclosed delimiter explanation
                .verify(|opened: &u32| *opened == 0),
            )
            .parse_next(i)?;

            // take everything consumed
            let mut offset = i.offset_from(&start);
            i.reset(&start);
            offset -= 1;
            let take = i.next_slice(offset);

            trace("closing", cut_err(closing)).parse_next(i)?;
            Ok(take)
        })
        .parse_next(i)
    }
}

/// implementation of a prat parsing table for use with the bash extended tests

/// An unary operator.
pub struct Unary<V, Q: Ord + Copy> {
    value: V,
    precedence: Q,
}

/// A binary operator.
pub struct Binary<V, Q: Ord + Copy> {
    value: V,
    precedence: Q,
    assoc: Assoc,
}

/// A single evaluation step.
pub enum Operation<P1, P2, P3, O> {
    /// A prefix operation.
    Prefix(P1, O),
    /// A postfix operation.
    Postfix(O, P2),
    /// A binary operation.
    Binary(O, P3, O),
}

/// Associativity for binary operators.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Assoc {
    /// Left associative.
    Left,
    /// Right associative.
    Right,
}

/// Element for operator stack.
enum Operator<P1, P2, P3, Q: Ord + Copy> {
    Prefix(P1, Q),
    Postfix(P2, Q),
    Binary(P3, Q, Assoc),
}

impl<P1, P2, P3, Q> Operator<P1, P2, P3, Q>
where
    Q: Ord + Copy,
{
    fn precedence(&self) -> Q {
        match self {
            Operator::Prefix(_, p) => *p,
            Operator::Postfix(_, p) => *p,
            Operator::Binary(_, p, _) => *p,
        }
    }

    fn is_postfix(&self) -> bool {
        match self {
            Operator::Postfix(_, _) => true,
            _ => false,
        }
    }
}

/// Runs the inner parser and transforms the result into an unary operator with the given
/// precedence.
///
/// Intended for use with [precedence].
/// # Arguments
/// * `precedence` The precedence of the operator.
/// * `parser` The parser to apply.
pub fn unary_op<Input, Output, Error, ParseNext, Precedence>(
    precedence: Precedence,
    mut parser: ParseNext,
) -> impl Parser<Input, Unary<Output, Precedence>, Error>
where
    Input: Stream,
    Error: ParserError<Input>,
    ParseNext: Parser<Input, Output, Error>,
    Precedence: Ord + Copy,
{
    trace("unary_op", move |input: &mut Input| {
        parser
            .parse_next(input)
            .map(|value| Unary { value, precedence })
    })
}

/// Runs the inner parser and transforms the result into a binary operator with the given precedence
/// and associativity.
///
/// Intended for use with [precedence].
/// # Arguments
/// * `precedence` The precedence of the operator.
/// * `assoc` The associativity of the operator.
/// * `parser` The parser to apply.
pub fn binary_op<Input, Output, Error, ParseNext, Precedence>(
    precedence: Precedence,
    assoc: Assoc,
    mut parser: ParseNext,
) -> impl Parser<Input, Binary<Output, Precedence>, Error>
where
    Input: Stream,
    Error: ParserError<Input>,
    ParseNext: Parser<Input, Output, Error>,
    Precedence: Ord + Copy,
{
    trace("unary_op", move |input: &mut Input| {
        parser.parse_next(input).map(|value| Binary {
            value,
            precedence,
            assoc,
        })
    })
}

pub fn precedence<
    Input,
    Output,
    Error,
    EXT,
    ParseOperand,
    Fold,
    ParsePrefix,
    ParseBinary,
    ParsePostfix,
    P1,
    P2,
    P3,
    Q,
>(
    mut prefix: ParsePrefix,
    mut postfix: ParsePostfix,
    mut binary: ParseBinary,
    mut operand: ParseOperand,
    mut fold: Fold,
) -> impl Parser<Input, Output, Error>
where
    Input: Stream,
    ParseOperand: Parser<Input, Output, Error>,
    Fold: FnMut(Operation<P1, P2, P3, Output>) -> Result<Output, EXT>,
    Error: ParserError<Input>, //+ winnow::error::FromExternalError<Input, EXT>,
    ParsePrefix: Parser<Input, Unary<P1, Q>, Error>,
    ParsePostfix: Parser<Input, Unary<P2, Q>, Error>,
    ParseBinary: Parser<Input, Binary<P3, Q>, Error>,
    Q: Ord + Copy,
{
    move |i: &mut Input| {
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        'main: loop {
            'prefix: loop {
                let start = i.checkpoint();
                let len = i.eof_offset();
                match prefix.parse_next(i) {
                    Err(ErrMode::Backtrack(_)) => {
                        i.reset(&start);
                        break 'prefix;
                    }
                    Err(e) => return Err(e),
                    Ok(op) => {
                        // infinite loop check: the parser must always consume
                        if i.eof_offset() == len {
                            return Err(ErrMode::assert(
                                i,
                                "`precedence` parsers must always consume",
                            ));
                        }
                        operators.push(Operator::Prefix(op.value, op.precedence));
                    }
                }
            }

            let start = i.checkpoint();
            let op = match operand.parse_next(i) {
                Ok(op) => op,
                Err(ErrMode::Backtrack(e)) => {
                    // TODO: error handling
                    return Err(ErrMode::Backtrack(e.append(i, &start, ErrorKind::Fail)));
                }
                Err(e) => return Err(e),
            };
            operands.push(op);

            'postfix: loop {
                let start = i.checkpoint();
                let len = i.eof_offset();
                match postfix.parse_next(i) {
                    Err(ErrMode::Backtrack(_)) => {
                        i.reset(&start);
                        break 'postfix;
                    }
                    Err(e) => return Err(e),
                    Ok(op) => {
                        // infinite loop check: the parser must always consume
                        if i.eof_offset() == len {
                            return Err(ErrMode::assert(
                                i,
                                "`precedence` parsers must always consume",
                            ));
                        }

                        while operators
                            .last()
                            .map(|lhs| lhs.precedence() <= op.precedence)
                            .unwrap_or(false)
                        {
                            let value = operands.pop().unwrap();
                            let operation = match operators.pop().unwrap() {
                                Operator::Prefix(op, _) => Operation::Prefix(op, value),
                                Operator::Postfix(op, _) => Operation::Postfix(value, op),
                                Operator::Binary(op, _, _) => match operands.pop() {
                                    Some(lhs) => Operation::Binary(lhs, op, value),
                                    None => {
                                        // TODO: proper error
                                        return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                                    }
                                },
                            };
                            let result = match fold(operation) {
                                Err(e) => {
                                    // TODO: error
                                    return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                                    // return Err(ErrMode::Backtrack(
                                    //     Error::from_external_error(i, ErrorKind::Fail, e),
                                    // ));
                                }
                                Ok(r) => r,
                            };
                            operands.push(result);
                        }
                        operators.push(Operator::Postfix(op.value, op.precedence));
                    }
                }
            }

            let start = i.checkpoint();
            let len = i.eof_offset();
            match binary.parse_next(i) {
                Err(ErrMode::Backtrack(_)) => {
                    i.reset(&start);
                    break 'main;
                }
                Err(e) => return Err(e),
                Ok(op) => {
                    while operators
                        .last()
                        .map(|lhs| {
                            lhs.precedence() < op.precedence
                                || (op.assoc == Assoc::Left && lhs.precedence() == op.precedence)
                                || (lhs.is_postfix())
                        })
                        .unwrap_or(false)
                    {
                        let value = operands.pop().unwrap();
                        let operation = match operators.pop().unwrap() {
                            Operator::Prefix(op, _) => Operation::Prefix(op, value),
                            Operator::Postfix(op, _) => Operation::Postfix(value, op),
                            Operator::Binary(op, _, _) => match operands.pop() {
                                Some(lhs) => Operation::Binary(lhs, op, value),
                                None => {
                                    // TODO: proper error
                                    return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                                }
                            },
                        };
                        let result = match fold(operation) {
                            Err(e) => {
                                return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                                // TODO: error
                                // return Err(ErrMode::Backtrack(Error::from_external_error(
                                //     i,
                                //     ErrorKind::Fail,
                                //     e,
                                // )));
                            }
                            Ok(r) => r,
                        };
                        operands.push(result);
                    }
                    operators.push(Operator::Binary(op.value, op.precedence, op.assoc));
                }
            }

            if i.eof_offset() == len {
                return Err(ErrMode::assert(
                    i,
                    "`precedence` either operand or operator must consume input",
                ));
            }
        }

        while operators.len() > 0 {
            let value = match operands.pop() {
                Some(o) => o,
                None => {
                    // TODO: proper error
                    return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                }
            };
            let operation = match operators.pop().unwrap() {
                Operator::Prefix(op, _) => Operation::Prefix(op, value),
                Operator::Postfix(op, _) => Operation::Postfix(value, op),
                Operator::Binary(op, _, _) => match operands.pop() {
                    Some(lhs) => Operation::Binary(lhs, op, value),
                    None => {
                        // TODO: proper error
                        return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                    }
                },
            };
            let result = match fold(operation) {
                Ok(r) => r,
                Err(e) => {
                    return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
                    // TODO: error
                    // return Err(ErrMode::Backtrack(Error::from_external_error(
                    //     i,
                    //     ErrorKind::Fail,
                    //     e,
                    // )));
                }
            };
            operands.push(result);
        }

        if operands.len() == 1 {
            return Ok(operands.pop().unwrap());
        } else {
            // TODO: proper error
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::parser2::tests::input;
    use crate::parser2::tests::Result;

    use super::*;
    #[test]
    fn test_take_inside() -> Result<()> {
        // let i = input(r#"(aaa(aaa) a(aa))"#);
        // let r = take_inside(b'(', b')').parse(i)?;
        // dbg!(std::str::from_utf8(r).unwrap());

        let i = input(r#"`1111`1` 222 `333`444  `"#);
        let r = take_inside(b'`', b'`').parse(i)?;
        dbg!(std::str::from_utf8(r).unwrap());
        Ok(())
    }
}
