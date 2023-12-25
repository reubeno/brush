use anyhow::Result;
use parser::ast;

use crate::Shell;

pub trait Evaluatable {
    fn eval(&self, shell: &mut Shell) -> Result<i64>;
}

impl Evaluatable for ast::ArithmeticExpr {
    fn eval(&self, shell: &mut Shell) -> Result<i64> {
        let value = match self {
            ast::ArithmeticExpr::Literal(l) => *l,
            ast::ArithmeticExpr::Reference(lvalue) => deref_lvalue(shell, lvalue)?,
            ast::ArithmeticExpr::UnaryOp(op, operand) => {
                let operand_eval = operand.eval(shell)?;
                apply_unary_op(shell, *op, operand_eval)
            }
            ast::ArithmeticExpr::BinaryOp(op, left, right) => {
                apply_binary_op(*op, left.eval(shell)?, right.eval(shell)?)
            }
            ast::ArithmeticExpr::Conditional(condition, then_expr, else_expr) => {
                let conditional_eval = condition.eval(shell)?;
                if conditional_eval != 0 {
                    then_expr.eval(shell)?
                } else {
                    else_expr.eval(shell)?
                }
            }
            ast::ArithmeticExpr::Assignment(lvalue, expr) => {
                let expr_eval = expr.eval(shell)?;
                assign(shell, lvalue, expr_eval)?
            }
            ast::ArithmeticExpr::BinaryAssignment(op, lvalue, operand) => {
                let value =
                    apply_binary_op(*op, deref_lvalue(shell, lvalue)?, operand.eval(shell)?);
                assign(shell, lvalue, value)?
            }
        };

        Ok(value)
    }
}

#[allow(clippy::unnecessary_wraps)]
fn deref_lvalue(shell: &mut Shell, lvalue: &ast::ArithmeticTarget) -> Result<i64> {
    match lvalue {
        ast::ArithmeticTarget::Variable(name) => {
            let value_str: String = shell
                .env
                .get(name)
                .map_or_else(String::new, |v| (&v.value).into());

            let value: i64 = value_str.parse().unwrap_or(0);
            Ok(value)
        }
        ast::ArithmeticTarget::ArrayElement(_, _) => todo!("deref array element"),
    }
}

fn apply_unary_op(_shell: &mut Shell, op: ast::UnaryOperator, _operand: i64) -> i64 {
    match op {
        ast::UnaryOperator::PostfixIncrement => todo!("post-increment"),
        ast::UnaryOperator::PostfixDecrement => todo!("post-decrement"),
        ast::UnaryOperator::UnaryPlus => todo!("unary plus"),
        ast::UnaryOperator::UnaryMinus => todo!("unary minus"),
        ast::UnaryOperator::PrefixIncrement => todo!("pre-increment"),
        ast::UnaryOperator::PrefixDecrement => todo!("pre-decrement"),
        ast::UnaryOperator::BitwiseNot => todo!("bitwise not"),
        ast::UnaryOperator::LogicalNot => todo!("logical not"),
    }
}

fn apply_binary_op(op: ast::BinaryOperator, left: i64, right: i64) -> i64 {
    match op {
        ast::BinaryOperator::Power => todo!("power"),
        ast::BinaryOperator::Multiply => left * right,
        ast::BinaryOperator::Divide => todo!("divide"),
        ast::BinaryOperator::Modulo => todo!("modulo"),
        ast::BinaryOperator::Comma => right,
        ast::BinaryOperator::Add => left + right,
        ast::BinaryOperator::Subtract => left - right,
        ast::BinaryOperator::ShiftLeft => left << right,
        ast::BinaryOperator::ShiftRight => left >> right,
        ast::BinaryOperator::LessThan => bool_to_i64(left < right),
        ast::BinaryOperator::LessThanOrEqualTo => bool_to_i64(left <= right),
        ast::BinaryOperator::GreaterThan => bool_to_i64(left > right),
        ast::BinaryOperator::GreaterThanOrEqualTo => bool_to_i64(left >= right),
        ast::BinaryOperator::Equals => bool_to_i64(left == right),
        ast::BinaryOperator::NotEquals => bool_to_i64(left != right),
        ast::BinaryOperator::BitwiseAnd => left & right,
        ast::BinaryOperator::BitwiseXor => left ^ right,
        ast::BinaryOperator::BitwiseOr => left | right,
        // TODO: check if these should short-circuit
        ast::BinaryOperator::LogicalAnd => bool_to_i64((left != 0) && (right != 0)),
        ast::BinaryOperator::LogicalOr => bool_to_i64((left != 0) || (right != 0)),
    }
}

fn assign(_shell: &mut Shell, lvalue: &ast::ArithmeticTarget, _value: i64) -> Result<i64> {
    match lvalue {
        ast::ArithmeticTarget::Variable(_) => todo!("assign variable"),
        ast::ArithmeticTarget::ArrayElement(_, _) => todo!("assign array element"),
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}
