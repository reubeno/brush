//! Parser for shell arithmetic expressions.

#[cfg(feature = "winnow-parser")]
use winnow::ascii::multispace0;
#[cfg(feature = "winnow-parser")]
use winnow::combinator::{Infix, Postfix, Prefix};
#[cfg(feature = "winnow-parser")]
use winnow::combinator::{
    alt, cut_err, delimited, expression, fail, not, opt, peek, separated_pair,
};
#[cfg(feature = "winnow-parser")]
use winnow::dispatch;
#[cfg(feature = "winnow-parser")]
use winnow::error::{ContextError, ErrMode};
#[cfg(feature = "winnow-parser")]
use winnow::prelude::*;
#[cfg(feature = "winnow-parser")]
use winnow::token::{any, one_of, take, take_while};

use crate::ast;
use crate::error;
use crate::parser::ParserImpl;

/// Parses a shell arithmetic expression using the default parser implementation.
///
/// # Arguments
///
/// * `input` - The arithmetic expression to parse, in string form.
pub fn parse(input: &str) -> Result<ast::ArithmeticExpr, error::WordParseError> {
    parse_with(input, ParserImpl::default())
}

/// Parses a shell arithmetic expression using the specified parser implementation.
///
/// # Arguments
///
/// * `input` - The arithmetic expression to parse, in string form.
/// * `impl_` - The parser implementation to use.
pub fn parse_with(
    input: &str,
    impl_: ParserImpl,
) -> Result<ast::ArithmeticExpr, error::WordParseError> {
    match impl_ {
        ParserImpl::Peg => cacheable_peg_parse(input.to_owned()),
        #[cfg(feature = "winnow-parser")]
        ParserImpl::Winnow => cacheable_winnow_parse(input.to_owned()),
    }
}

// ============================================================================
// PEG-based implementation
// ============================================================================

#[cached::proc_macro::cached(size = 64, result = true)]
fn cacheable_peg_parse(input: String) -> Result<ast::ArithmeticExpr, error::WordParseError> {
    tracing::debug!(target: "arithmetic", "parsing arithmetic expression (peg): '{input}'");
    peg_arithmetic::full_expression(input.as_str())
        .map_err(|e| error::WordParseError::ArithmeticExpression(e.to_string()))
}

peg::parser! {
    grammar peg_arithmetic() for str {
        pub(crate) rule full_expression() -> ast::ArithmeticExpr =
            ![_] { ast::ArithmeticExpr::Literal(0) } /
            _ e:expression() _ { e }

        pub(crate) rule expression() -> ast::ArithmeticExpr = precedence!{
            x:(@) _ "," _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Comma, Box::new(x), Box::new(y)) }
            --
            x:lvalue() _ "*=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Multiply, x, Box::new(y)) }
            x:lvalue() _ "/=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Divide, x, Box::new(y)) }
            x:lvalue() _ "%=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Modulo, x, Box::new(y)) }
            x:lvalue() _ "+=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Add, x, Box::new(y)) }
            x:lvalue() _ "-=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Subtract, x, Box::new(y)) }
            x:lvalue() _ "<<=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::ShiftLeft, x, Box::new(y)) }
            x:lvalue() _ ">>=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::ShiftRight, x, Box::new(y)) }
            x:lvalue() _ "&=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseAnd, x, Box::new(y)) }
            x:lvalue() _ "|=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseOr, x, Box::new(y)) }
            x:lvalue() _ "^=" _ y:(@) { ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseXor, x, Box::new(y)) }
            x:lvalue() _ "=" _ y:(@) { ast::ArithmeticExpr::Assignment(x, Box::new(y)) }
            --
            x:@ _ "?" _ y:expression() _ ":" _ z:(@) { ast::ArithmeticExpr::Conditional(Box::new(x), Box::new(y), Box::new(z)) }
            --
            x:(@) _ "||" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LogicalOr, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "&&" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LogicalAnd, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "|" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseOr, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "^" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseXor, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "&" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseAnd, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "==" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Equals, Box::new(x), Box::new(y)) }
            x:(@) _ "!=" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::NotEquals, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "<" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LessThan, Box::new(x), Box::new(y)) }
            x:(@) _ ">" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::GreaterThan, Box::new(x), Box::new(y)) }
            x:(@) _ "<=" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LessThanOrEqualTo, Box::new(x), Box::new(y)) }
            x:(@) _ ">=" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::GreaterThanOrEqualTo, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "<<" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::ShiftLeft, Box::new(x), Box::new(y)) }
            x:(@) _ ">>" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::ShiftRight, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "+" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Add, Box::new(x), Box::new(y)) }
            x:(@) _ "-" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Subtract, Box::new(x), Box::new(y)) }
            --
            x:(@) _ "*" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Multiply, Box::new(x), Box::new(y)) }
            x:(@) _ "%" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Modulo, Box::new(x), Box::new(y)) }
            x:(@) _ "/" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Divide, Box::new(x), Box::new(y)) }
            --
            x:@ _ "**" _ y:(@) { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Power, Box::new(x), Box::new(y)) }
            --
            "!" _ x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::LogicalNot, Box::new(x)) }
            "~" _ x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::BitwiseNot, Box::new(x)) }
            --
            // NOTE: We add negative lookahead to avoid ambiguity with the pre-increment/pre-decrement operators.
            "+" !['+'] _ x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryPlus, Box::new(x)) }
            "-" !['-'] _ x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryMinus, Box::new(x)) }
            --
            "++" _ x:lvalue() { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PrefixIncrement, x) }
            "--" _ x:lvalue() { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PrefixDecrement, x) }
            --
            x:lvalue() _ "++" { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PostfixIncrement, x) }
            x:lvalue() _ "--" { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PostfixDecrement, x) }
            --
            n:literal_number() { ast::ArithmeticExpr::Literal(n) }
            l:lvalue() { ast::ArithmeticExpr::Reference(l) }
            "(" _ expr:expression() _ ")" { expr }
        }

        rule lvalue() -> ast::ArithmeticTarget =
            name:variable_name() "[" index:expression() "]" {
                ast::ArithmeticTarget::ArrayElement(name.to_owned(), Box::new(index))
            } /
            name:variable_name() {
                ast::ArithmeticTarget::Variable(name.to_owned())
            }

        rule variable_name() -> &'input str =
            $(['a'..='z' | 'A'..='Z' | '_'](['a'..='z' | 'A'..='Z' | '_' | '0'..='9']*))

        rule _() -> () = quiet!{[' ' | '\t' | '\n' | '\r']*} {}

        rule literal_number() -> i64 =
            // Literal with explicit radix (format: <base>#<literal>)
            radix:decimal_literal() "#" s:$(['0'..='9' | 'a'..='z' | 'A'..='Z' | '@' | '_']+) {?
                parse_shell_literal_number(s, radix.cast_unsigned())
            } /
            // Hex literal
            "0" ['x' | 'X'] s:$(['0'..='9' | 'a'..='f' | 'A'..='F']*) {?
                i64::from_str_radix(s, 16).or(Err("i64"))
            } /
            // Octal literal
            s:$("0" ['0'..='8']*) {?
                i64::from_str_radix(s, 8).or(Err("i64"))
            } /
            // Decimal literal
            decimal_literal()

        rule decimal_literal() -> i64 =
            s:$(['1'..='9'] ['0'..='9']*) {?
                // Parse as u64 first, then cast to i64. This handles values like
                // 9223372036854775808 (i64::MAX + 1) which is needed for INT64_MIN
                // when preceded by unary minus: -(9223372036854775808) wraps to i64::MIN.
                s.parse::<u64>().map(|v| v.cast_signed()).or(Err("i64"))
            }
    }
}

// ============================================================================
// Winnow Pratt-based implementation
// ============================================================================

#[cfg(feature = "winnow-parser")]
#[cached::proc_macro::cached(size = 64, result = true)]
fn cacheable_winnow_parse(input: String) -> Result<ast::ArithmeticExpr, error::WordParseError> {
    tracing::debug!(target: "arithmetic", "parsing arithmetic expression (winnow): '{input}'");
    winnow_full_expression
        .parse(input.as_str())
        .map_err(|e| error::WordParseError::ArithmeticExpression(e.to_string()))
}

#[cfg(feature = "winnow-parser")]
fn winnow_full_expression(i: &mut &str) -> ModalResult<ast::ArithmeticExpr> {
    alt((
        winnow::combinator::eof.value(ast::ArithmeticExpr::Literal(0)),
        delimited(multispace0, pratt_expr(0), multispace0),
    ))
    .parse_next(i)
}

/// Convert an expression to an assignment target (lvalue), failing if not a reference.
#[cfg(feature = "winnow-parser")]
fn expr_to_target(expr: ast::ArithmeticExpr) -> ModalResult<ast::ArithmeticTarget> {
    match expr {
        ast::ArithmeticExpr::Reference(target) => Ok(target),
        _ => Err(ErrMode::Backtrack(ContextError::default())),
    }
}

#[cfg(feature = "winnow-parser")]
fn variable_name<'i>(i: &mut &'i str) -> ModalResult<&'i str> {
    (
        one_of(|c: char| c.is_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(i)
}

#[cfg(feature = "winnow-parser")]
fn lvalue_atom<'i>(i: &mut &'i str) -> ModalResult<ast::ArithmeticExpr> {
    let name = variable_name(i)?;
    let index = opt(delimited('[', pratt_expr(0), cut_err(']'))).parse_next(i)?;
    Ok(match index {
        Some(idx) => ast::ArithmeticExpr::Reference(ast::ArithmeticTarget::ArrayElement(
            name.to_owned(),
            Box::new(idx),
        )),
        None => ast::ArithmeticExpr::Reference(ast::ArithmeticTarget::Variable(name.to_owned())),
    })
}

#[cfg(feature = "winnow-parser")]
fn hex_literal(i: &mut &str) -> ModalResult<i64> {
    let _ = ('0', one_of(['x', 'X'])).parse_next(i)?;
    let digits = take_while(1.., |c: char| c.is_ascii_hexdigit()).parse_next(i)?;
    i64::from_str_radix(digits, 16).map_err(|_| ErrMode::Backtrack(ContextError::default()))
}

#[cfg(feature = "winnow-parser")]
fn octal_literal(i: &mut &str) -> ModalResult<i64> {
    let s = ('0', take_while(0.., |c: char| matches!(c, '0'..='7')))
        .take()
        .parse_next(i)?;
    i64::from_str_radix(s, 8).map_err(|_| ErrMode::Backtrack(ContextError::default()))
}

#[cfg(feature = "winnow-parser")]
fn decimal_literal_winnow(i: &mut &str) -> ModalResult<i64> {
    let s = (
        one_of(|c: char| c.is_ascii_digit() && c != '0'),
        take_while(0.., |c: char| c.is_ascii_digit()),
    )
        .take()
        .parse_next(i)?;
    s.parse::<u64>()
        .map(|v| v as i64)
        .map_err(|_| ErrMode::Backtrack(ContextError::default()))
}

#[cfg(feature = "winnow-parser")]
fn base_literal(i: &mut &str) -> ModalResult<i64> {
    let radix = decimal_literal_winnow.parse_next(i)?;
    '#'.parse_next(i)?;
    let digits =
        take_while(1.., |c: char| c.is_alphanumeric() || c == '@' || c == '_').parse_next(i)?;
    parse_shell_literal_number(digits, radix as u64)
        .map_err(|_| ErrMode::Backtrack(ContextError::default()))
}

#[cfg(feature = "winnow-parser")]
fn literal_number(i: &mut &str) -> ModalResult<i64> {
    alt((
        base_literal,
        hex_literal,
        octal_literal,
        decimal_literal_winnow,
    ))
    .parse_next(i)
}

/// Pratt expression parser with configurable minimum precedence level.
#[cfg(feature = "winnow-parser")]
fn pratt_expr<'i>(
    precedence: i64,
) -> impl Parser<&'i str, ast::ArithmeticExpr, ErrMode<ContextError>> {
    move |i: &mut &'i str| {
        expression(
            // Atom: an operand, optionally surrounded by whitespace.
            delimited(
                multispace0,
                dispatch! {peek(any);
                    '(' => delimited('(', pratt_expr(0), cut_err(')')),
                    _ => alt((
                        literal_number.map(ast::ArithmeticExpr::Literal),
                        lvalue_atom,
                    ))
                },
                multispace0,
            ),
        )
        .current_precedence_level(precedence)
        // Prefix operators (tried before the atom)
        .prefix(delimited(
            multispace0,
            alt((
                // Two-char prefix: ++ and --
                dispatch! {take(2usize);
                    "++" => Prefix(17, |_: &mut _, a| {
                        let t = expr_to_target(a)?;
                        Ok(ast::ArithmeticExpr::UnaryAssignment(
                            ast::UnaryAssignmentOperator::PrefixIncrement, t,
                        ))
                    }),
                    "--" => Prefix(17, |_: &mut _, a| {
                        let t = expr_to_target(a)?;
                        Ok(ast::ArithmeticExpr::UnaryAssignment(
                            ast::UnaryAssignmentOperator::PrefixDecrement, t,
                        ))
                    }),
                    _ => fail,
                },
                // Single-char prefix: !, ~, unary +, unary -
                dispatch! {any;
                    '!' => not('=').value(Prefix(15, |_: &mut _, a| {
                        Ok(ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::LogicalNot, Box::new(a)))
                    })),
                    '~' => Prefix(15, |_: &mut _, a| {
                        Ok(ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::BitwiseNot, Box::new(a)))
                    }),
                    '+' => not('+').value(Prefix(16, |_: &mut _, a| {
                        Ok(ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryPlus, Box::new(a)))
                    })),
                    '-' => not('-').value(Prefix(16, |_: &mut _, a| {
                        Ok(ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryMinus, Box::new(a)))
                    })),
                    _ => fail,
                },
            )),
            multispace0,
        ))
        // Postfix operators (tried after the atom)
        .postfix(delimited(
            multispace0,
            alt((
                // Two-char postfix: ++ and --
                dispatch! {take(2usize);
                    "++" => Postfix(18, |_: &mut _, a| {
                        let t = expr_to_target(a)?;
                        Ok(ast::ArithmeticExpr::UnaryAssignment(
                            ast::UnaryAssignmentOperator::PostfixIncrement, t,
                        ))
                    }),
                    "--" => Postfix(18, |_: &mut _, a| {
                        let t = expr_to_target(a)?;
                        Ok(ast::ArithmeticExpr::UnaryAssignment(
                            ast::UnaryAssignmentOperator::PostfixDecrement, t,
                        ))
                    }),
                    _ => fail,
                },
                // Ternary: ? then : else
                dispatch! {any;
                    '?' => Postfix(3, |i: &mut &'i str, cond| {
                        let (then_e, else_e) = cut_err(separated_pair(
                            pratt_expr(0),
                            delimited(multispace0, ':', multispace0),
                            pratt_expr(3),
                        ))
                        .parse_next(i)?;
                        Ok(ast::ArithmeticExpr::Conditional(
                            Box::new(cond), Box::new(then_e), Box::new(else_e),
                        ))
                    }),
                    _ => fail,
                },
            )),
            multispace0,
        ))
        // Infix operators
        .infix(alt((
            // Three-char compound assignments: <<= and >>=
            dispatch! {take(3usize);
                "<<=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::ShiftLeft, t, Box::new(b)))
                }),
                ">>=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::ShiftRight, t, Box::new(b)))
                }),
                _ => fail,
            },
            // Two-char infix operators
            dispatch! {take(2usize);
                "**" => Infix::Right(14, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Power, Box::new(a), Box::new(b)))
                }),
                "||" => Infix::Left(4, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LogicalOr, Box::new(a), Box::new(b)))
                }),
                "&&" => Infix::Left(5, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LogicalAnd, Box::new(a), Box::new(b)))
                }),
                "==" => Infix::Left(9, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Equals, Box::new(a), Box::new(b)))
                }),
                "!=" => Infix::Left(9, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::NotEquals, Box::new(a), Box::new(b)))
                }),
                "<=" => Infix::Left(10, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LessThanOrEqualTo, Box::new(a), Box::new(b)))
                }),
                ">=" => Infix::Left(10, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::GreaterThanOrEqualTo, Box::new(a), Box::new(b)))
                }),
                "<<" => Infix::Left(11, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::ShiftLeft, Box::new(a), Box::new(b)))
                }),
                ">>" => Infix::Left(11, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::ShiftRight, Box::new(a), Box::new(b)))
                }),
                "*=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Multiply, t, Box::new(b)))
                }),
                "/=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Divide, t, Box::new(b)))
                }),
                "%=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Modulo, t, Box::new(b)))
                }),
                "+=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Add, t, Box::new(b)))
                }),
                "-=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::Subtract, t, Box::new(b)))
                }),
                "&=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseAnd, t, Box::new(b)))
                }),
                "|=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseOr, t, Box::new(b)))
                }),
                "^=" => Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::BinaryAssignment(ast::BinaryOperator::BitwiseXor, t, Box::new(b)))
                }),
                _ => fail,
            },
            // Single-char infix operators (with guards to avoid ambiguity with multi-char ops)
            dispatch! {any;
                ',' => Infix::Left(1, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Comma, Box::new(a), Box::new(b)))
                }),
                '=' => not('=').value(Infix::Right(2, |_: &mut _, a, b| {
                    let t = expr_to_target(a)?;
                    Ok(ast::ArithmeticExpr::Assignment(t, Box::new(b)))
                })),
                '|' => not('|').value(Infix::Left(6, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseOr, Box::new(a), Box::new(b)))
                })),
                '^' => Infix::Left(7, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseXor, Box::new(a), Box::new(b)))
                }),
                '&' => not('&').value(Infix::Left(8, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::BitwiseAnd, Box::new(a), Box::new(b)))
                })),
                '<' => not(one_of(['<', '='])).value(Infix::Left(10, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::LessThan, Box::new(a), Box::new(b)))
                })),
                '>' => not(one_of(['>', '='])).value(Infix::Left(10, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::GreaterThan, Box::new(a), Box::new(b)))
                })),
                '+' => not(one_of(['+', '='])).value(Infix::Left(12, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Add, Box::new(a), Box::new(b)))
                })),
                '-' => not(one_of(['-', '='])).value(Infix::Left(12, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Subtract, Box::new(a), Box::new(b)))
                })),
                '*' => not(one_of(['*', '='])).value(Infix::Left(13, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Multiply, Box::new(a), Box::new(b)))
                })),
                '/' => not('=').value(Infix::Left(13, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Divide, Box::new(a), Box::new(b)))
                })),
                '%' => not('=').value(Infix::Left(13, |_: &mut _, a, b| {
                    Ok(ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Modulo, Box::new(a), Box::new(b)))
                })),
                _ => fail,
            },
        )))
        .parse_next(i)
    }
}

// ============================================================================
// Shared utilities
// ============================================================================

fn parse_shell_literal_number(s: &str, radix: u64) -> Result<i64, &'static str> {
    if !(2..=64).contains(&radix) {
        return Err("invalid base");
    }

    // For bases <= 36: case-insensitive (a-z and A-Z both map to 10-35)
    // For bases > 36 (bash extension):
    //   0-9 = 0-9, a-z = 10-35, A-Z = 36-61, @ = 62, _ = 63
    let mut result: i64 = 0;

    for ch in s.chars() {
        let digit_val = if radix <= 36 {
            match ch {
                '0'..='9' => (ch as u64) - ('0' as u64),
                'a'..='z' => (ch as u64) - ('a' as u64) + 10,
                'A'..='Z' => (ch as u64) - ('A' as u64) + 10,
                _ => return Err("invalid digit"),
            }
        } else {
            match ch {
                '0'..='9' => (ch as u64) - ('0' as u64),
                'a'..='z' => (ch as u64) - ('a' as u64) + 10,
                'A'..='Z' => (ch as u64) - ('A' as u64) + 36,
                '@' => 62,
                '_' => 63,
                _ => return Err("invalid digit"),
            }
        };

        if digit_val >= radix {
            return Err("value too great for base");
        }

        result = result
            .wrapping_mul(radix.cast_signed())
            .wrapping_add(digit_val.cast_signed());
    }

    Ok(result)
}
