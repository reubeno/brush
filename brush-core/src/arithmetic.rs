//! Arithmetic evaluation

use std::borrow::Cow;

use crate::{ExecutionParameters, Shell, env, expansion, variables};
use brush_parser::ast;

/// Represents an error that occurs during evaluation of an arithmetic expression.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// Division by zero.
    #[error("division by zero")]
    DivideByZero,

    /// Negative exponent.
    #[error("exponent less than 0")]
    NegativeExponent,

    /// Failed to tokenize an arithmetic expression.
    #[error("failed to tokenize expression")]
    FailedToTokenizeExpression,

    /// Failed to expand an arithmetic expression.
    #[error("failed to expand expression: '{0}'")]
    FailedToExpandExpression(String),

    /// Failed to access an element of an array.
    #[error("failed to access array")]
    FailedToAccessArray,

    /// Failed to update the shell environment in an assignment operator.
    #[error("failed to update environment")]
    FailedToUpdateEnvironment,

    /// Failed to parse an arithmetic expression.
    #[error("failed to parse expression: '{0}'")]
    ParseError(String),

    /// Failed to trace an arithmetic expression.
    #[error("failed tracing expression")]
    TraceError,
}

/// Trait implemented by arithmetic expressions that can be evaluated.
pub(crate) trait ExpandAndEvaluate {
    /// Evaluate the given expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    /// * `trace_if_needed` - Whether to trace the evaluation.
    async fn eval(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
        trace_if_needed: bool,
    ) -> Result<i64, EvalError>;
}

impl ExpandAndEvaluate for ast::UnexpandedArithmeticExpr {
    async fn eval(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
        trace_if_needed: bool,
    ) -> Result<i64, EvalError> {
        expand_and_eval(shell, params, self.value.as_str(), trace_if_needed).await
    }
}

/// Evaluate the given arithmetic expression, returning the resulting numeric value.
///
/// # Arguments
///
/// * `shell` - The shell to use for evaluation.
/// * `expr` - The unexpanded arithmetic expression to evaluate.
/// * `trace_if_needed` - Whether to trace the evaluation.
pub(crate) async fn expand_and_eval(
    shell: &mut Shell,
    params: &ExecutionParameters,
    expr: &str,
    trace_if_needed: bool,
) -> Result<i64, EvalError> {
    // Per documentation, first shell-expand it.
    let expanded_self = expansion::basic_expand_str_without_tilde(shell, params, expr)
        .await
        .map_err(|_e| EvalError::FailedToExpandExpression(expr.to_owned()))?;

    // Now parse.
    let expr = brush_parser::arithmetic::parse(&expanded_self)
        .map_err(|_e| EvalError::ParseError(expanded_self))?;

    // Trace if applicable.
    if trace_if_needed && shell.options.print_commands_and_arguments {
        shell
            .trace_command(params, std::format!("(( {expr} ))"))
            .await
            .map_err(|_err| EvalError::TraceError)?;
    }

    // Now evaluate.
    expr.eval(shell)
}

/// Trait implemented by evaluatable arithmetic expressions.
pub trait Evaluatable {
    /// Evaluate the given arithmetic expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError>;
}

impl Evaluatable for ast::ArithmeticExpr {
    fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError> {
        let value = match self {
            Self::Literal(l) => *l,
            Self::Reference(lvalue) => deref_lvalue(shell, lvalue)?,
            Self::UnaryOp(op, operand) => apply_unary_op(shell, *op, operand)?,
            Self::BinaryOp(op, left, right) => apply_binary_op(shell, *op, left, right)?,
            Self::Conditional(condition, then_expr, else_expr) => {
                let conditional_eval = condition.eval(shell)?;

                // Ensure we only evaluate the branch indicated by the condition.
                if conditional_eval != 0 {
                    then_expr.eval(shell)?
                } else {
                    else_expr.eval(shell)?
                }
            }
            Self::Assignment(lvalue, expr) => {
                let expr_eval = expr.eval(shell)?;
                assign(shell, lvalue, expr_eval)?
            }
            Self::UnaryAssignment(op, lvalue) => apply_unary_assignment_op(shell, lvalue, *op)?,
            Self::BinaryAssignment(op, lvalue, operand) => {
                let value = apply_binary_op(shell, *op, &Self::Reference(lvalue.clone()), operand)?;
                assign(shell, lvalue, value)?
            }
        };

        Ok(value)
    }
}

fn deref_lvalue(shell: &mut Shell, lvalue: &ast::ArithmeticTarget) -> Result<i64, EvalError> {
    let value_str: Cow<'_, str> = match lvalue {
        ast::ArithmeticTarget::Variable(name) => shell.env_str(name).unwrap_or(Cow::Borrowed("")),
        ast::ArithmeticTarget::ArrayElement(name, index_expr) => {
            let index_str = index_expr.eval(shell)?.to_string();

            shell
                .env
                .get(name)
                .map_or_else(
                    || Ok(None),
                    |(_, v)| v.value().get_at(index_str.as_str(), shell),
                )
                .map_err(|_err| EvalError::FailedToAccessArray)?
                .unwrap_or(Cow::Borrowed(""))
        }
    };

    let parsed_value = brush_parser::arithmetic::parse(value_str.as_ref())
        .map_err(|_err| EvalError::ParseError(value_str.to_string()))?;

    parsed_value.eval(shell)
}

fn apply_unary_op(
    shell: &mut Shell,
    op: ast::UnaryOperator,
    operand: &ast::ArithmeticExpr,
) -> Result<i64, EvalError> {
    let operand_eval = operand.eval(shell)?;

    match op {
        ast::UnaryOperator::UnaryPlus => Ok(operand_eval),
        ast::UnaryOperator::UnaryMinus => Ok(-operand_eval),
        ast::UnaryOperator::BitwiseNot => Ok(!operand_eval),
        ast::UnaryOperator::LogicalNot => Ok(bool_to_i64(operand_eval == 0)),
    }
}

fn apply_binary_op(
    shell: &mut Shell,
    op: ast::BinaryOperator,
    left: &ast::ArithmeticExpr,
    right: &ast::ArithmeticExpr,
) -> Result<i64, EvalError> {
    // First, special-case short-circuiting operators. For those, we need
    // to ensure we don't eagerly evaluate both operands. After we
    // get these out of the way, we can easily just evaluate operands
    // for the other operators.
    match op {
        ast::BinaryOperator::LogicalAnd => {
            let left = left.eval(shell)?;
            if left == 0 {
                return Ok(bool_to_i64(false));
            }

            let right = right.eval(shell)?;
            return Ok(bool_to_i64(right != 0));
        }
        ast::BinaryOperator::LogicalOr => {
            let left = left.eval(shell)?;
            if left != 0 {
                return Ok(bool_to_i64(true));
            }

            let right = right.eval(shell)?;
            return Ok(bool_to_i64(right != 0));
        }
        _ => (),
    }

    // The remaining operators unconditionally operate both operands.
    let left = left.eval(shell)?;
    let right = right.eval(shell)?;

    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_sign_loss)]
    match op {
        ast::BinaryOperator::Power => {
            if right >= 0 {
                Ok(wrapping_pow_u64(left, right as u64))
            } else {
                Err(EvalError::NegativeExponent)
            }
        }
        ast::BinaryOperator::Multiply => Ok(left.wrapping_mul(right)),
        ast::BinaryOperator::Divide => {
            if right == 0 {
                Err(EvalError::DivideByZero)
            } else {
                Ok(left.wrapping_div(right))
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
        ast::BinaryOperator::Add => Ok(left.wrapping_add(right)),
        ast::BinaryOperator::Subtract => Ok(left.wrapping_sub(right)),
        ast::BinaryOperator::ShiftLeft => Ok(left.wrapping_shl(right as u32)),
        ast::BinaryOperator::ShiftRight => Ok(left.wrapping_shr(right as u32)),
        ast::BinaryOperator::LessThan => Ok(bool_to_i64(left < right)),
        ast::BinaryOperator::LessThanOrEqualTo => Ok(bool_to_i64(left <= right)),
        ast::BinaryOperator::GreaterThan => Ok(bool_to_i64(left > right)),
        ast::BinaryOperator::GreaterThanOrEqualTo => Ok(bool_to_i64(left >= right)),
        ast::BinaryOperator::Equals => Ok(bool_to_i64(left == right)),
        ast::BinaryOperator::NotEquals => Ok(bool_to_i64(left != right)),
        ast::BinaryOperator::BitwiseAnd => Ok(left & right),
        ast::BinaryOperator::BitwiseXor => Ok(left ^ right),
        ast::BinaryOperator::BitwiseOr => Ok(left | right),
        ast::BinaryOperator::LogicalAnd => unreachable!("LogicalAnd covered above"),
        ast::BinaryOperator::LogicalOr => unreachable!("LogicalOr covered above"),
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

fn assign(shell: &mut Shell, lvalue: &ast::ArithmeticTarget, value: i64) -> Result<i64, EvalError> {
    match lvalue {
        ast::ArithmeticTarget::Variable(name) => {
            shell
                .env
                .update_or_add(
                    name.as_str(),
                    variables::ShellValueLiteral::Scalar(value.to_string()),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )
                .map_err(|_err| EvalError::FailedToUpdateEnvironment)?;
        }
        ast::ArithmeticTarget::ArrayElement(name, index_expr) => {
            let index_str = index_expr.eval(shell)?.to_string();

            shell
                .env
                .update_or_add_array_element(
                    name.as_str(),
                    index_str,
                    value.to_string(),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )
                .map_err(|_err| EvalError::FailedToUpdateEnvironment)?;
        }
    }

    Ok(value)
}

const fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

// N.B. We implement our own version of wrapping_pow that takes a 64-bit exponent.
// This seems to be the best way to guarantee that we handle overflow cases
// with exponents correctly.
const fn wrapping_pow_u64(mut base: i64, mut exponent: u64) -> i64 {
    let mut result: i64 = 1;

    while exponent > 0 {
        if exponent % 2 == 1 {
            result = result.wrapping_mul(base);
        }

        base = base.wrapping_mul(base);
        exponent /= 2;
    }

    result
}
