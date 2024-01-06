use anyhow::Result;
use parser::ast;

use crate::Shell;

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("division by zero")]
    DivideByZero,
}

pub trait Evaluatable {
    fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError>;
}

impl Evaluatable for ast::ArithmeticExpr {
    fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError> {
        let value = match self {
            ast::ArithmeticExpr::Literal(l) => *l,
            ast::ArithmeticExpr::Reference(lvalue) => deref_lvalue(shell, lvalue)?,
            ast::ArithmeticExpr::UnaryOp(op, operand) => {
                let operand_eval = operand.eval(shell)?;
                apply_unary_op(shell, *op, operand_eval)?
            }
            ast::ArithmeticExpr::BinaryOp(op, left, right) => {
                apply_binary_op(*op, left.eval(shell)?, right.eval(shell)?)?
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
                    apply_binary_op(*op, deref_lvalue(shell, lvalue)?, operand.eval(shell)?)?;
                assign(shell, lvalue, value)?
            }
        };

        Ok(value)
    }
}

#[allow(clippy::unnecessary_wraps)]
fn deref_lvalue(shell: &mut Shell, lvalue: &ast::ArithmeticTarget) -> Result<i64, EvalError> {
    match lvalue {
        ast::ArithmeticTarget::Variable(name) => {
            let value_str: String = shell
                .env
                .get(name)
                .map_or_else(String::new, |v| (&v.value).into());

            let value: i64 = value_str.parse().unwrap_or(0);
            Ok(value)
        }
        ast::ArithmeticTarget::ArrayElement(_, _) => todo!("UNIMPLEMENTED: deref array element"),
    }
}

#[allow(clippy::unnecessary_wraps)]
fn apply_unary_op(
    _shell: &mut Shell,
    op: ast::UnaryOperator,
    operand: i64,
) -> Result<i64, EvalError> {
    match op {
        ast::UnaryOperator::PostfixIncrement => todo!("UNIMPLEMENTED: post-increment"),
        ast::UnaryOperator::PostfixDecrement => todo!("UNIMPLEMENTED: post-decrement"),
        ast::UnaryOperator::UnaryPlus => todo!("UNIMPLEMENTED: unary plus"),
        ast::UnaryOperator::UnaryMinus => todo!("UNIMPLEMENTED: unary minus"),
        ast::UnaryOperator::PrefixIncrement => todo!("UNIMPLEMENTED: pre-increment"),
        ast::UnaryOperator::PrefixDecrement => todo!("UNIMPLEMENTED: pre-decrement"),
        ast::UnaryOperator::BitwiseNot => Ok(!operand),
        ast::UnaryOperator::LogicalNot => todo!("UNIMPLEMENTED: logical not"),
    }
}

fn apply_binary_op(op: ast::BinaryOperator, left: i64, right: i64) -> Result<i64, EvalError> {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    match op {
        ast::BinaryOperator::Power => Ok(left.pow(right as u32)),
        ast::BinaryOperator::Multiply => Ok(left * right),
        ast::BinaryOperator::Divide => {
            if right == 0 {
                Err(EvalError::DivideByZero)
            } else {
                Ok(left / right)
            }
        }
        ast::BinaryOperator::Modulo => {
            if right == 0 {
                Err(EvalError::DivideByZero)
            } else {
                Ok(left % right)
            }
        }
        ast::BinaryOperator::Comma => Ok(right),
        ast::BinaryOperator::Add => Ok(left + right),
        ast::BinaryOperator::Subtract => Ok(left - right),
        ast::BinaryOperator::ShiftLeft => Ok(left << right),
        ast::BinaryOperator::ShiftRight => Ok(left >> right),
        ast::BinaryOperator::LessThan => Ok(bool_to_i64(left < right)),
        ast::BinaryOperator::LessThanOrEqualTo => Ok(bool_to_i64(left <= right)),
        ast::BinaryOperator::GreaterThan => Ok(bool_to_i64(left > right)),
        ast::BinaryOperator::GreaterThanOrEqualTo => Ok(bool_to_i64(left >= right)),
        ast::BinaryOperator::Equals => Ok(bool_to_i64(left == right)),
        ast::BinaryOperator::NotEquals => Ok(bool_to_i64(left != right)),
        ast::BinaryOperator::BitwiseAnd => Ok(left & right),
        ast::BinaryOperator::BitwiseXor => Ok(left ^ right),
        ast::BinaryOperator::BitwiseOr => Ok(left | right),
        // TODO: check if these should short-circuit
        ast::BinaryOperator::LogicalAnd => Ok(bool_to_i64((left != 0) && (right != 0))),
        ast::BinaryOperator::LogicalOr => Ok(bool_to_i64((left != 0) || (right != 0))),
    }
}

fn assign(
    _shell: &mut Shell,
    lvalue: &ast::ArithmeticTarget,
    _value: i64,
) -> Result<i64, EvalError> {
    match lvalue {
        ast::ArithmeticTarget::Variable(_) => todo!("UNIMPLEMENTED: assign variable"),
        ast::ArithmeticTarget::ArrayElement(_, _) => todo!("UNIMPLEMENTED: assign array element"),
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}
