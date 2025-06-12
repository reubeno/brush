use crate::{ExecutionParameters, Shell, error, extendedtests};

pub(crate) fn eval_test_expr(
    expr: &brush_parser::ast::TestExpr,
    shell: &mut Shell,
    params: &ExecutionParameters,
) -> Result<bool, error::Error> {
    match expr {
        brush_parser::ast::TestExpr::False => Ok(false),
        brush_parser::ast::TestExpr::Literal(s) => Ok(!s.is_empty()),
        brush_parser::ast::TestExpr::And(left, right) => {
            Ok(eval_test_expr(left, shell, params)? && eval_test_expr(right, shell, params)?)
        }
        brush_parser::ast::TestExpr::Or(left, right) => {
            Ok(eval_test_expr(left, shell, params)? || eval_test_expr(right, shell, params)?)
        }
        brush_parser::ast::TestExpr::Not(expr) => Ok(!eval_test_expr(expr, shell, params)?),
        brush_parser::ast::TestExpr::Parenthesized(expr) => eval_test_expr(expr, shell, params),
        brush_parser::ast::TestExpr::UnaryTest(op, operand) => {
            extendedtests::apply_unary_predicate_to_str(op, operand, shell, params)
        }
        brush_parser::ast::TestExpr::BinaryTest(op, left, right) => {
            extendedtests::apply_binary_predicate_to_strs(op, left.as_str(), right.as_str(), shell)
        }
    }
}
