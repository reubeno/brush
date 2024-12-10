use std::marker::PhantomData;

use winnow::error::ContextError;
use winnow::stream::{Stream, StreamIsPartial};
use winnow::Parser;

use super::Input;

struct Neither<O>(PhantomData<O>);

struct Unary<F, O>(F, PhantomData<O>)
where
    F: FnMut(O) -> O;

struct Binary<F, O>(F, PhantomData<O>)
where
    F: FnMut(O, O) -> O;

trait OperandType {
    type T;
}
impl<F, O> OperandType for Unary<F, O>
where
    F: FnMut(O) -> O,
{
    type T = O;
}
impl<F, O> OperandType for Binary<F, O>
where
    F: FnMut(O, O) -> O,
{
    type T = O;
}
impl<O> OperandType for Neither<O> {
    type T = O;
}

struct PrecedenceInner<Input, _OperatorParser, _OperatorOutput>
where
    Input: Stream + StreamIsPartial,
    _OperatorParser: Parser<Input, _OperatorOutput, ContextError>,
{
    parser: _OperatorParser,
    precedence: PrecedenceKind,
    assoc: Assoc,
    _phantom: PhantomData<Input>,
    _phantom2: PhantomData<_OperatorOutput>,
}

struct Precedence<Operator, Input, _OperatorParser, _OperatorOutput>
where
    Input: Stream + StreamIsPartial,
    _OperatorParser: Parser<Input, _OperatorOutput, ContextError>,
{
    op: Operator,
    inner: PrecedenceInner<Input, _OperatorParser, _OperatorOutput>,
}

impl<Operand, Operator, Input, OperatorParser, _ParserOutput>
    Precedence<Operator, Input, OperatorParser, _ParserOutput>
where
    Operator: OperandType<T = Operand>,
    Input: Stream + StreamIsPartial,
    OperatorParser: Parser<Input, _ParserOutput, ContextError>,
{
    fn new(op: Operator, precedence: PrecedenceKind, parser: OperatorParser) -> Self {
        Self {
            op,
            inner: PrecedenceInner {
                parser,
                precedence,
                assoc: Assoc::Neither,
                _phantom: PhantomData::<Input>,
                _phantom2: PhantomData::<_ParserOutput>,
            },
        }
    }

    #[inline(always)]
    fn unary<F>(self, f: F) -> Precedence<Unary<F, Operand>, Input, OperatorParser, _ParserOutput>
    where
        F: FnMut(Operand) -> Operand,
    {
        Precedence {
            op: Unary(f, PhantomData::<Operand>),
            inner: self.inner,
        }
    }

    #[inline(always)]
    fn binary<F>(self, f: F) -> Precedence<Binary<F, Operand>, Input, OperatorParser, _ParserOutput>
    where
        F: FnMut(Operand, Operand) -> Operand,
    {
        Precedence {
            op: Binary(f, PhantomData::<Operand>),
            inner: self.inner,
        }
    }
    #[inline(always)]
    fn left(mut self, strength: u32) -> Self {
        self.inner.assoc = Assoc::Left(strength);
        Self {
            op: self.op,
            inner: self.inner,
        }
    }
    #[inline(always)]
    fn right(mut self, strength: u32) -> Self {
        self.inner.assoc = Assoc::Right(strength);
        Self {
            op: self.op,
            inner: self.inner,
        }
    }
}

#[derive(Clone, Copy)]
enum PrecedenceKind {
    Prefix,
    Infix,
    Postfix,
    Nilfix,
}

#[derive(Clone, Copy)]
enum Assoc {
    Right(u32),
    Left(u32),
    Neither,
}

#[inline(always)]
fn prefix<Input, Operand, OperatorParser, ParserOutput>(
    parser: OperatorParser,
) -> Precedence<Neither<Operand>, Input, OperatorParser, ParserOutput>
where
    Input: Stream + StreamIsPartial,
    OperatorParser: Parser<Input, ParserOutput, ContextError>,
{
    Precedence::new(
        Neither::<Operand>(PhantomData::<Operand>),
        PrecedenceKind::Prefix,
        parser,
    )
}

#[inline(always)]
fn infix<Input, Operand, OperatorParser, _ParserOutput>(
    parser: OperatorParser,
) -> Precedence<Neither<Operand>, Input, OperatorParser, _ParserOutput>
where
    Input: Stream + StreamIsPartial,
    OperatorParser: Parser<Input, _ParserOutput, ContextError>,
{
    Precedence::new(
        Neither::<Operand>(PhantomData::<Operand>),
        PrecedenceKind::Infix,
        parser,
    )
}
#[inline(always)]
fn postfix<Input, Operand, OperatorParser, _ParserOutput>(
    parser: OperatorParser,
) -> Precedence<Neither<Operand>, Input, OperatorParser, _ParserOutput>
where
    Input: Stream + StreamIsPartial,
    OperatorParser: Parser<Input, _ParserOutput, ContextError>,
{
    Precedence::new(
        Neither::<Operand>(PhantomData::<Operand>),
        PrecedenceKind::Postfix,
        parser,
    )
}

trait ApplyPrecedence<Input, Operand> {}

impl<Operator, Operand, Input, _OperatorParser, _OperatorOutput> ApplyPrecedence<Input, Operand>
    for Precedence<Operator, Input, _OperatorParser, _OperatorOutput>
where
    Operator: OperandType<T = Operand>,
    Input: Stream + StreamIsPartial,
    _OperatorParser: Parser<Input, _OperatorOutput, ContextError>,
{
}

fn precedence<'i, ParseOperand, Operand>(
    parser: ParseOperand,
    ops: (
        impl ApplyPrecedence<Input<'i>, Operand>,
        impl ApplyPrecedence<Input<'i>, Operand>,
    ),
) -> impl Parser<Input<'i>, Operand, ContextError>
where
    ParseOperand: Parser<Input<'i>, Operand, ContextError>,
{
    parser
}

#[cfg(test)]
mod tests {
    use winnow::ascii::digit1;
    use winnow::token::literal;
    use winnow::PResult;

    use crate::parser2::new_input;
    use crate::ParserOptions;

    use super::*;

    // NOTE: "+".prefix() if a bad design. it pollutes parser namespace with pratt domain
    // functions

    #[test]
    fn test_api() {
        // "+".prefix().left(2)
        // unary('-').prefix().left(2)

        fn parse() -> PResult<()> {
            let mut i = new_input(ParserOptions::default(), "1");
            precedence(
                digit1.map(|_| 1usize),
                (
                    prefix("-").right(1),
                    infix("+").binary(|a, b| a + b).left(0),
                ),
            )
            .parse_next(&mut i)?;
            Ok(())
        }
        parse().unwrap();

        // precedence(digit1,
        //  (
        //      prefix("-").right(1).unary(|a| - b)
        //      infix("+").left(0).binary(|a, b| a + b)
        //      prefix("*").left(3).binary(|a, b| a * b)
        //      prefix("/").left(3).binary(|a, b| a / b)
        //      prefix("!").unary(|a| !a)
        //  )
        // )
        // precedence("111", vec![prefix(left(2, "+"))]);

        // precedence();
    }
}
// assoc neither
// https://github.com/segeljakt/pratt/issues/2

// let calc = pratt(
//     digits1.map(Expr::Int),
//     (
//         '-'.prefix(Right(1), |r| unary(r, Op::Neg));
//         '+'.infix(Left(0), |l, r| binary(l, r, Op::Add));
//         '!'.prefix(Right(3), |r| unary(r, Op::Fact));
//     )
// );
