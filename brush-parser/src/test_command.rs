//! Parser for shell test commands.

//
// TODO(IMSTR): figure out how to convert this over.
//

use crate::{ast, error};

/// Parses a test command expression.
///
/// # Arguments
///
/// * `input` - The test command expression to parse, in string form.
pub fn parse(input: &[String]) -> Result<ast::TestExpr, error::TestCommandParseError> {
    let input: Vec<_> = input.iter().map(|s| s.as_str()).collect();

    let expr = test_command::full_expression(input.as_slice())
        .map_err(error::TestCommandParseError::TestCommand)?;

    Ok(expr)
}

peg::parser! {
    grammar test_command<'a>() for [&'a str] {
        pub(crate) rule full_expression() -> ast::TestExpr =
            end() { ast::TestExpr::False } /
            e:one_arg_expr() end() { e } /
            e:two_arg_expr() end()  { e } /
            e:three_arg_expr() end()  { e } /
            e:four_arg_expr() end()  { e } /
            expression()

        rule one_arg_expr() -> ast::TestExpr =
            [s] { ast::TestExpr::Literal(s.into()) }

        rule two_arg_expr() -> ast::TestExpr =
            ["!"] e:one_arg_expr() { ast::TestExpr::Not(Box::from(e)) } /
            op:unary_op() [s] { ast::TestExpr::UnaryTest(op, s.into()) } /
            [_] [_] { ast::TestExpr::False }

        rule three_arg_expr() -> ast::TestExpr =
            [left] ["-a"] [right] { ast::TestExpr::And(Box::from(ast::TestExpr::Literal(left.into())), Box::from(ast::TestExpr::Literal(right.into()))) } /
            [left] ["-o"] [right] { ast::TestExpr::Or(Box::from(ast::TestExpr::Literal(left.into())), Box::from(ast::TestExpr::Literal(right.into()))) } /
            [left] op:binary_op() [right] { ast::TestExpr::BinaryTest(op, left.into(), right.into()) } /
            ["!"] e:two_arg_expr() { ast::TestExpr::Not(Box::from(e)) } /
            ["("] e:one_arg_expr() [")"] { e } /
            [_] [_] [_] { ast::TestExpr::False }

        rule four_arg_expr() -> ast::TestExpr =
            ["!"] e:three_arg_expr() { ast::TestExpr::Not(Box::from(e)) }

        rule expression() -> ast::TestExpr = precedence! {
            left:(@) ["-a"] right:@ { ast::TestExpr::And(Box::from(left), Box::from(right)) }
            left:(@) ["-o"] right:@ { ast::TestExpr::Or(Box::from(left), Box::from(right)) }
            --
            ["("] e:expression() [")"] { ast::TestExpr::Parenthesized(Box::from(e)) }
            --
            ["!"] e:@ { ast::TestExpr::Not(Box::from(e)) }
            --
            [left] op:binary_op() [right] { ast::TestExpr::BinaryTest(op, left.into(), right.into()) }
            --
            op:unary_op() [operand] { ast::TestExpr::UnaryTest(op, operand.into()) }
            --
            [s] { ast::TestExpr::Literal(s.into()) }
        }

        rule unary_op() -> ast::UnaryPredicate =
            ["-a"] { ast::UnaryPredicate::FileExists } /
            ["-b"] { ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile } /
            ["-c"] { ast::UnaryPredicate::FileExistsAndIsCharSpecialFile } /
            ["-d"] { ast::UnaryPredicate::FileExistsAndIsDir } /
            ["-e"] { ast::UnaryPredicate::FileExists } /
            ["-f"] { ast::UnaryPredicate::FileExistsAndIsRegularFile } /
            ["-g"] { ast::UnaryPredicate::FileExistsAndIsSetgid } /
            ["-h"] { ast::UnaryPredicate::FileExistsAndIsSymlink } /
            ["-k"] { ast::UnaryPredicate::FileExistsAndHasStickyBit } /
            ["-n"] { ast::UnaryPredicate::StringHasNonZeroLength } /
            ["-o"] { ast::UnaryPredicate::ShellOptionEnabled } /
            ["-p"] { ast::UnaryPredicate::FileExistsAndIsFifo } /
            ["-r"] { ast::UnaryPredicate::FileExistsAndIsReadable } /
            ["-s"] { ast::UnaryPredicate::FileExistsAndIsNotZeroLength } /
            ["-t"] { ast::UnaryPredicate::FdIsOpenTerminal } /
            ["-u"] { ast::UnaryPredicate::FileExistsAndIsSetuid } /
            ["-v"] { ast::UnaryPredicate::ShellVariableIsSetAndAssigned } /
            ["-w"] { ast::UnaryPredicate::FileExistsAndIsWritable } /
            ["-x"] { ast::UnaryPredicate::FileExistsAndIsExecutable } /
            ["-z"] { ast::UnaryPredicate::StringHasZeroLength } /
            ["-G"] { ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId } /
            ["-L"] { ast::UnaryPredicate::FileExistsAndIsSymlink } /
            ["-N"] { ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead } /
            ["-O"] { ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId } /
            ["-R"] { ast::UnaryPredicate::ShellVariableIsSetAndNameRef } /
            ["-S"] { ast::UnaryPredicate::FileExistsAndIsSocket }

        rule binary_op() -> ast::BinaryPredicate =
            ["=="] { ast::BinaryPredicate::StringExactlyMatchesPattern } /
            ["-ef"] { ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers } /
            ["-eq"] { ast::BinaryPredicate::ArithmeticEqualTo } /
            ["-ge"] { ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo } /
            ["-gt"] { ast::BinaryPredicate::ArithmeticGreaterThan } /
            ["-le"] { ast::BinaryPredicate::ArithmeticLessThanOrEqualTo } /
            ["-lt"] { ast::BinaryPredicate::ArithmeticLessThan } /
            ["-ne"] { ast::BinaryPredicate::ArithmeticNotEqualTo } /
            ["-nt"] { ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot } /
            ["-ot"] { ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes } /
            ["=="] { ast::BinaryPredicate::StringExactlyMatchesPattern } /
            ["="] { ast::BinaryPredicate::StringExactlyMatchesPattern } /
            ["!="] { ast::BinaryPredicate::StringDoesNotExactlyMatchPattern } /
            ["<"] { ast::BinaryPredicate::LeftSortsBeforeRight } /
            [">"] { ast::BinaryPredicate::LeftSortsAfterRight }

        rule end() = ![_]
    }
}
