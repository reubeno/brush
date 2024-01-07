use anyhow::Result;
use parser::ast;

use crate::{env, Shell};

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
            ast::ArithmeticExpr::UnaryAssignment(op, lvalue) => {
                apply_unary_assignment_op(shell, lvalue, *op)?
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
        ast::UnaryOperator::UnaryPlus => Ok(operand),
        ast::UnaryOperator::UnaryMinus => Ok(-operand),
        ast::UnaryOperator::BitwiseNot => Ok(!operand),
        ast::UnaryOperator::LogicalNot => Ok(bool_to_i64(operand != 0)),
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

fn apply_unary_assignment_op(
    shell: &mut Shell,
    lvalue: &ast::ArithmeticTarget,
    op: ast::UnaryAssignmentOperator,
) -> Result<i64, EvalError> {
    let value = deref_lvalue(shell, lvalue)?;

    match op {
        ast::UnaryAssignmentOperator::PrefixIncrement => {
            let new_value = value + 1;
            assign(shell, lvalue, new_value)?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PrefixDecrement => {
            let new_value = value - 1;
            assign(shell, lvalue, new_value)?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PostfixIncrement => {
            let new_value = value + 1;
            assign(shell, lvalue, new_value)?;
            Ok(value)
        }
        ast::UnaryAssignmentOperator::PostfixDecrement => {
            let new_value = value - 1;
            assign(shell, lvalue, new_value)?;
            Ok(value)
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn assign(shell: &mut Shell, lvalue: &ast::ArithmeticTarget, value: i64) -> Result<i64, EvalError> {
    match lvalue {
        ast::ArithmeticTarget::Variable(name) => {
            shell
                .env
                .update_or_add(
                    name.as_str(),
                    value.to_string().as_str(),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )
                .unwrap();
            Ok(value)
        }
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
