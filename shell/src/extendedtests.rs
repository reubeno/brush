use anyhow::Result;
use parser::ast;

use crate::{expansion::expand_word, patterns, Shell};

pub(crate) fn eval_expression(expr: &ast::ExtendedTestExpr, shell: &mut Shell) -> Result<bool> {
    #[allow(clippy::single_match_else)]
    match expr {
        ast::ExtendedTestExpr::UnaryTest(op, operand) => {
            let expanded_operand = expand_word(shell, operand)?;
            apply_unary_predicate(op, expanded_operand.as_str())
        }
        ast::ExtendedTestExpr::BinaryTest(op, left, right) => {
            let expanded_left = expand_word(shell, left)?;
            let expanded_right = expand_word(shell, right)?;
            apply_binary_predicate(op, expanded_left.as_str(), expanded_right.as_str())
        }
        ast::ExtendedTestExpr::And(left, right) => {
            let result = eval_expression(left, shell)? && eval_expression(right, shell)?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Or(left, right) => {
            let result = eval_expression(left, shell)? || eval_expression(right, shell)?;
            Ok(result)
        }
        _ => {
            // TODO: implement eval_expression
            log::error!("UNIMPLEMENTED: eval test expression: {:?}", expr);
            Ok(true)
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn apply_unary_predicate(op: &ast::UnaryPredicate, operand: &str) -> Result<bool> {
    #[allow(clippy::match_single_binding)]
    match op {
        ast::UnaryPredicate::StringHasNonZeroLength => Ok(!operand.is_empty()),
        ast::UnaryPredicate::StringHasZeroLength => Ok(operand.is_empty()),
        _ => {
            log::error!("UNIMPLEMENTED: extended test unary predicate: {op:?}(\"{operand}\")");
            Ok(true)
        }
    }
}

fn apply_binary_predicate(op: &ast::BinaryPredicate, left: &str, right: &str) -> Result<bool> {
    #[allow(clippy::single_match_else)]
    match op {
        ast::BinaryPredicate::StringsAreEqual => {
            let s = left;
            let pattern = right;
            patterns::pattern_matches(pattern, s)
        }
        ast::BinaryPredicate::StringsNotEqual => {
            let s = left;
            let pattern = right;
            let eq = patterns::pattern_matches(pattern, s)?;
            Ok(!eq)
        }
        ast::BinaryPredicate::ArithmeticGreaterThan => {
            let left: Result<i64, _> = left.parse();
            let right: Result<i64, _> = right.parse();

            if let (Ok(left), Ok(right)) = (left, right) {
                Ok(left > right)
            } else {
                Ok(false)
            }
        }
        _ => {
            log::error!(
                "UNIMPLEMENTED: extended test binary predicate: {op:?}(\"{left}\",\"{right}\")"
            );
            Ok(true)
        }
    }
}
