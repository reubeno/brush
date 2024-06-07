use crate::{error, extendedtests, Shell};

pub(crate) fn eval_test_expr(
    expr: &brush_parser::ast::TestExpr,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    match expr {
        brush_parser::ast::TestExpr::False => Ok(false),
        brush_parser::ast::TestExpr::Literal(s) => Ok(!s.is_empty()),
        brush_parser::ast::TestExpr::And(left, right) => {
            Ok(eval_test_expr(left, shell)? && eval_test_expr(right, shell)?)
        }
        brush_parser::ast::TestExpr::Or(left, right) => {
            Ok(eval_test_expr(left, shell)? || eval_test_expr(right, shell)?)
        }
        brush_parser::ast::TestExpr::Not(expr) => Ok(!eval_test_expr(expr, shell)?),
        brush_parser::ast::TestExpr::Parenthesized(expr) => eval_test_expr(expr, shell),
        brush_parser::ast::TestExpr::UnaryTest(op, operand) => {
            extendedtests::apply_unary_predicate_to_str(op, operand, shell)
        }
        brush_parser::ast::TestExpr::BinaryTest(op, left, right) => {
            extendedtests::apply_binary_predicate_to_strs(op, left.as_str(), right.as_str(), shell)
        }
    }
}
