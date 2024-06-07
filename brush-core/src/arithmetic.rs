use std::borrow::Cow;

use crate::{env, expansion, variables, Shell};
use brush_parser::ast;

/// Represents an error that occurs during evaluation of an arithmetic expression.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// Division by zero.
    #[error("division by zero")]
    DivideByZero,

    /// Failed to tokenize an arithmetic expression.
    #[error("failed to tokenize expression")]
    FailedToTokenizeExpression,

    /// Failed to expand an arithmetic expression.
    #[error("failed to expand expression")]
    FailedToExpandExpression,

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
#[async_trait::async_trait]
pub trait ExpandAndEvaluate {
    /// Evaluate the given expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    /// * `trace_if_needed` - Whether to trace the evaluation.
    async fn eval(&self, shell: &mut Shell, trace_if_needed: bool) -> Result<i64, EvalError>;
}

#[async_trait::async_trait]
impl ExpandAndEvaluate for ast::UnexpandedArithmeticExpr {
    async fn eval(&self, shell: &mut Shell, trace_if_needed: bool) -> Result<i64, EvalError> {
        // Per documentation, first shell-expand it.
        let expanded_self = expansion::basic_expand_str_without_tilde(shell, self.value.as_str())
            .await
            .map_err(|_e| EvalError::FailedToExpandExpression)?;

        // Now parse.
        let expr = brush_parser::parse_arithmetic_expression(&expanded_self)
            .map_err(|_e| EvalError::ParseError(expanded_self))?;

        // Trace if applicable.
        if trace_if_needed && shell.options.print_commands_and_arguments {
            shell
                .trace_command(std::format!("(( {expr} ))"))
                .map_err(|_err| EvalError::TraceError)?;
        }

        // Now evaluate.
        expr.eval(shell).await
    }
}

/// Trait implemented by evaluatable arithmetic expressions.
#[async_trait::async_trait]
pub trait Evaluatable {
    /// Evaluate the given arithmetic expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    async fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError>;
}

#[async_trait::async_trait]
impl Evaluatable for ast::ArithmeticExpr {
    async fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError> {
        let value = match self {
            ast::ArithmeticExpr::Literal(l) => *l,
            ast::ArithmeticExpr::Reference(lvalue) => deref_lvalue(shell, lvalue).await?,
            ast::ArithmeticExpr::UnaryOp(op, operand) => {
                apply_unary_op(shell, *op, operand).await?
            }
            ast::ArithmeticExpr::BinaryOp(op, left, right) => {
                apply_binary_op(shell, *op, left, right).await?
            }
            ast::ArithmeticExpr::Conditional(condition, then_expr, else_expr) => {
                let conditional_eval = condition.eval(shell).await?;

                // Ensure we only evaluate the branch indicated by the condition.
                if conditional_eval != 0 {
                    then_expr.eval(shell).await?
                } else {
                    else_expr.eval(shell).await?
                }
            }
            ast::ArithmeticExpr::Assignment(lvalue, expr) => {
                let expr_eval = expr.eval(shell).await?;
                assign(shell, lvalue, expr_eval).await?
            }
            ast::ArithmeticExpr::UnaryAssignment(op, lvalue) => {
                apply_unary_assignment_op(shell, lvalue, *op).await?
            }
            ast::ArithmeticExpr::BinaryAssignment(op, lvalue, operand) => {
                let value = apply_binary_op(
                    shell,
                    *op,
                    &ast::ArithmeticExpr::Reference(lvalue.clone()),
                    operand,
                )
                .await?;
                assign(shell, lvalue, value).await?
            }
        };

        Ok(value)
    }
}

async fn deref_lvalue(shell: &mut Shell, lvalue: &ast::ArithmeticTarget) -> Result<i64, EvalError> {
    let value_str: Cow<'_, str> = match lvalue {
        ast::ArithmeticTarget::Variable(name) => shell
            .env
            .get(name)
            .map_or_else(|| Cow::Borrowed(""), |(_, v)| v.value().to_cow_string()),
        ast::ArithmeticTarget::ArrayElement(name, index_expr) => {
            let index_str = index_expr.eval(shell).await?.to_string();

            shell
                .env
                .get(name)
                .map_or_else(|| Ok(None), |(_, v)| v.value().get_at(index_str.as_str()))
                .map_err(|_err| EvalError::FailedToAccessArray)?
                .unwrap_or(Cow::Borrowed(""))
        }
    };

    let value: i64 = value_str.parse().unwrap_or(0);
    Ok(value)
}

#[allow(clippy::unnecessary_wraps)]
async fn apply_unary_op(
    shell: &mut Shell,
    op: ast::UnaryOperator,
    operand: &ast::ArithmeticExpr,
) -> Result<i64, EvalError> {
    let operand_eval = operand.eval(shell).await?;

    match op {
        ast::UnaryOperator::UnaryPlus => Ok(operand_eval),
        ast::UnaryOperator::UnaryMinus => Ok(-operand_eval),
        ast::UnaryOperator::BitwiseNot => Ok(!operand_eval),
        ast::UnaryOperator::LogicalNot => Ok(bool_to_i64(operand_eval == 0)),
    }
}

async fn apply_binary_op(
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
            let left = left.eval(shell).await?;
            if left == 0 {
                return Ok(bool_to_i64(false));
            }

            let right = right.eval(shell).await?;
            return Ok(bool_to_i64(right != 0));
        }
        ast::BinaryOperator::LogicalOr => {
            let left = left.eval(shell).await?;
            if left != 0 {
                return Ok(bool_to_i64(true));
            }

            let right = right.eval(shell).await?;
            return Ok(bool_to_i64(right != 0));
        }
        _ => (),
    }

    // The remaining operators unconditionally operate both operands.
    let left = left.eval(shell).await?;
    let right = right.eval(shell).await?;

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
        ast::BinaryOperator::LogicalAnd => unreachable!("LogicalAnd covered above"),
        ast::BinaryOperator::LogicalOr => unreachable!("LogicalOr covered above"),
    }
}

async fn apply_unary_assignment_op(
    shell: &mut Shell,
    lvalue: &ast::ArithmeticTarget,
    op: ast::UnaryAssignmentOperator,
) -> Result<i64, EvalError> {
    let value = deref_lvalue(shell, lvalue).await?;

    match op {
        ast::UnaryAssignmentOperator::PrefixIncrement => {
            let new_value = value + 1;
            assign(shell, lvalue, new_value).await?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PrefixDecrement => {
            let new_value = value - 1;
            assign(shell, lvalue, new_value).await?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PostfixIncrement => {
            let new_value = value + 1;
            assign(shell, lvalue, new_value).await?;
            Ok(value)
        }
        ast::UnaryAssignmentOperator::PostfixDecrement => {
            let new_value = value - 1;
            assign(shell, lvalue, new_value).await?;
            Ok(value)
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
async fn assign(
    shell: &mut Shell,
    lvalue: &ast::ArithmeticTarget,
    value: i64,
) -> Result<i64, EvalError> {
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
            let index_str = index_expr.eval(shell).await?.to_string();

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

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}
