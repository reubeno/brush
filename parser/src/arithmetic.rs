use crate::ast;
use anyhow::Result;

pub fn parse_arithmetic_expression(input: &str) -> Result<ast::ArithmeticExpr> {
    log::debug!("parsing arithmetic expression: '{input}'");

    let expr = arithmetic::expression(input)?;
    Ok(expr)
}

peg::parser! {
    grammar arithmetic() for str {
        // TODO: fix associativity
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
            --
            x:lvalue() _ "=" _ y:(@) { ast::ArithmeticExpr::Assignment(x, Box::new(y)) }
            --
            // TODO: validate parens are in the right spot
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
            x:(@) _ "**" _ y:@ { ast::ArithmeticExpr::BinaryOp(ast::BinaryOperator::Power, Box::new(x), Box::new(y)) }
            --
            "!" x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::LogicalNot, Box::new(x)) }
            "~" x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::BitwiseNot, Box::new(x)) }
            --
            "++" x:lvalue() { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PrefixIncrement, x) }
            "--" x:lvalue() { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PrefixDecrement, x) }
            --
            x:lvalue() "++" { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PostfixIncrement, x) }
            x:lvalue() "--" { ast::ArithmeticExpr::UnaryAssignment(ast::UnaryAssignmentOperator::PostfixDecrement, x) }
            --
            "+" x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryPlus, Box::new(x)) }
            "-" x:(@) { ast::ArithmeticExpr::UnaryOp(ast::UnaryOperator::UnaryMinus, Box::new(x)) }
            --
            // TODO: What about parentheses?
            // TODO: Is this where literals and such should go?
            n:literal_number() { ast::ArithmeticExpr::Literal(n) }
            l:lvalue() { ast::ArithmeticExpr::Reference(l) }
        }

        rule lvalue() -> ast::ArithmeticTarget =
            name:variable_name() "[" index:expression() "]" {
                ast::ArithmeticTarget::ArrayElement(name.to_owned(), Box::new(index))
            } /
            name:variable_name() {
                ast::ArithmeticTarget::Variable(name.to_owned())
            }

        rule variable_name() -> &'input str =
            $(['a'..='z' | 'A'..='Z' | '_']+)

        rule _() -> () = quiet!{[' ' | '\t' | '\n' | '\r']*} {}

        rule literal_number() -> i64 =
            // TODO: handle binary?
            "0" ['x' | 'X'] s:$(['0'..='9']*) {? i64::from_str_radix(s, 16).or(Err("i64")) } /
            s:$("0" ['0'..='9']*) {? i64::from_str_radix(s, 8).or(Err("i64")) } /
            s:$(['1'..='9'] ['0'..='9']*) {? s.parse().or(Err("i64")) }
    }
}
