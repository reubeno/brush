use anyhow::Result;
use parser::ast;

use crate::{env, expansion, variables, Shell};

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("division by zero")]
    DivideByZero,

    #[error("failed to tokenize expression")]
    FailedToTokenizeExpression,

    #[error("failed to expand expression")]
    FailedToExpandExpression,

    #[error("failed to access array")]
    FailedToAccessArray,

    #[error("failed to update environment")]
    FailedToUpdateEnvironment,

    #[error("failed to parse expression: {0}")]
    ParseError(String),
}

#[async_trait::async_trait]
pub trait Evaluatable {
    async fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError>;
}

#[async_trait::async_trait]
impl Evaluatable for ast::UnexpandedArithmeticExpr {
    async fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError> {
        // Per documentation, first shell-expand it.
        let tokenized_self = parser::tokenize_str(self.value.as_str())
            .map_err(|_e| EvalError::FailedToTokenizeExpression)?;
        let mut expanded_self = String::new();

        for token in tokenized_self {
            match token {
                parser::Token::Word(value, _) => {
                    let expansion = expansion::basic_expand_word(shell, &ast::Word { value })
                        .await
                        .map_err(|_e| EvalError::FailedToExpandExpression)?;
                    expanded_self.push_str(expansion.as_str());
                }
                parser::Token::Operator(value, _) => expanded_self.push_str(value.as_str()),
            }
        }

        let expr = parser::parse_arithmetic_expression(&expanded_self)
            .map_err(|_e| EvalError::ParseError(expanded_self))?;
        expr.eval(shell).await
    }
}

#[async_trait::async_trait]
impl Evaluatable for ast::ArithmeticExpr {
    async fn eval(&self, shell: &mut Shell) -> Result<i64, EvalError> {
        let value = match self {
            ast::ArithmeticExpr::Literal(l) => *l,
            ast::ArithmeticExpr::Reference(lvalue) => deref_lvalue(shell, lvalue).await?,
            ast::ArithmeticExpr::UnaryOp(op, operand) => {
                let operand_eval = operand.eval(shell).await?;
                apply_unary_op(shell, *op, operand_eval)?
            }
            ast::ArithmeticExpr::BinaryOp(op, left, right) => {
                apply_binary_op(*op, left.eval(shell).await?, right.eval(shell).await?)?
            }
            ast::ArithmeticExpr::Conditional(condition, then_expr, else_expr) => {
                let conditional_eval = condition.eval(shell).await?;
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
                    *op,
                    deref_lvalue(shell, lvalue).await?,
                    operand.eval(shell).await?,
                )?;
                assign(shell, lvalue, value).await?
            }
        };

        Ok(value)
    }
}

async fn deref_lvalue(shell: &mut Shell, lvalue: &ast::ArithmeticTarget) -> Result<i64, EvalError> {
    let value_str: String = match lvalue {
        ast::ArithmeticTarget::Variable(name) => shell
            .env
            .get(name)
            .map_or_else(String::new, |v| v.value().into()),
        ast::ArithmeticTarget::ArrayElement(name, index_expr) => {
            let index_str = index_expr.eval(shell).await?.to_string();

            shell
                .env
                .get(name)
                .map_or_else(|| Ok(None), |v| v.value().get_at(index_str.as_str()))
                .map_err(|_err| EvalError::FailedToAccessArray)?
                .unwrap_or_else(String::new)
        }
    };

    let value: i64 = value_str.parse().unwrap_or(0);
    Ok(value)
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
        ast::UnaryOperator::LogicalNot => Ok(bool_to_i64(operand == 0)),
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
                    index_str.as_str(),
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
