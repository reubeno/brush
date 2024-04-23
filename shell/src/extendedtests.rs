use faccess::PathExt;
use parser::ast;
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
};

use crate::{
    env, error, expansion, patterns,
    variables::{self, ArrayLiteral},
    Shell,
};

#[async_recursion::async_recursion]
pub(crate) async fn eval_expression(
    expr: &ast::ExtendedTestExpr,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::single_match_else)]
    match expr {
        ast::ExtendedTestExpr::UnaryTest(op, operand) => {
            let expanded_operand = expansion::basic_expand_word(shell, operand).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {op} {expanded_operand} ]]"))?;
            }

            apply_unary_predicate(op, expanded_operand.as_str(), shell)
        }
        ast::ExtendedTestExpr::BinaryTest(op, left, right) => {
            let expanded_left = expansion::basic_expand_word(shell, left).await?;
            let expanded_right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {expanded_left} {op} {expanded_right} ]]"))?;
            }

            apply_binary_predicate(op, expanded_left.as_str(), expanded_right.as_str(), shell)
        }
        ast::ExtendedTestExpr::And(left, right) => {
            let result =
                eval_expression(left, shell).await? && eval_expression(right, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Or(left, right) => {
            let result =
                eval_expression(left, shell).await? || eval_expression(right, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Not(expr) => {
            let result = !eval_expression(expr, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Parenthesized(expr) => eval_expression(expr, shell).await,
    }
}

#[allow(clippy::unnecessary_wraps)]
fn apply_unary_predicate(
    op: &ast::UnaryPredicate,
    operand: &str,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::match_single_binding)]
    match op {
        ast::UnaryPredicate::StringHasNonZeroLength => Ok(!operand.is_empty()),
        ast::UnaryPredicate::StringHasZeroLength => Ok(operand.is_empty()),
        ast::UnaryPredicate::FileExists => {
            let path = Path::new(operand);
            Ok(path.exists())
        }
        ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile => {
            let path = Path::new(operand);
            Ok(try_get_file_type(path).map_or(false, |ft| ft.is_block_device()))
        }
        ast::UnaryPredicate::FileExistsAndIsCharSpecialFile => {
            let path = Path::new(operand);
            Ok(try_get_file_type(path).map_or(false, |ft| ft.is_char_device()))
        }
        ast::UnaryPredicate::FileExistsAndIsDir => {
            let path = Path::new(operand);
            Ok(path.is_dir())
        }
        ast::UnaryPredicate::FileExistsAndIsRegularFile => {
            let path = Path::new(operand);
            Ok(path.is_file())
        }
        ast::UnaryPredicate::FileExistsAndIsSetgid => {
            const S_ISGID: u32 = 0o2000;
            let path = Path::new(operand);
            let file_mode = try_get_file_mode(path);
            Ok(file_mode.map_or(false, |mode| mode & S_ISGID != 0))
        }
        ast::UnaryPredicate::FileExistsAndIsSymlink => {
            let path = Path::new(operand);
            Ok(path.is_symlink())
        }
        ast::UnaryPredicate::FileExistsAndHasStickyBit => {
            const S_ISVTX: u32 = 0o1000;
            let path = Path::new(operand);
            let file_mode = try_get_file_mode(path);
            Ok(file_mode.map_or(false, |mode| mode & S_ISVTX != 0))
        }
        ast::UnaryPredicate::FileExistsAndIsFifo => {
            let path = Path::new(operand);
            Ok(try_get_file_type(path).map_or(false, |ft: std::fs::FileType| ft.is_fifo()))
        }
        ast::UnaryPredicate::FileExistsAndIsReadable => {
            let path = Path::new(operand);
            Ok(path.readable())
        }
        ast::UnaryPredicate::FileExistsAndIsNotZeroLength => {
            let path = Path::new(operand);
            if let Ok(metadata) = path.metadata() {
                Ok(metadata.len() > 0)
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::FdIsOpenTerminal => {
            error::unimp("unary extended test predicate: FdIsOpenTerminal")
        }
        ast::UnaryPredicate::FileExistsAndIsSetuid => {
            const S_ISUID: u32 = 0o4000;
            let path = Path::new(operand);
            let file_mode = try_get_file_mode(path);
            Ok(file_mode.map_or(false, |mode| mode & S_ISUID != 0))
        }
        ast::UnaryPredicate::FileExistsAndIsWritable => {
            let path = Path::new(operand);
            Ok(path.writable())
        }
        ast::UnaryPredicate::FileExistsAndIsExecutable => {
            let path = Path::new(operand);
            Ok(path.executable())
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId => {
            error::unimp("unary extended test predicate: FileExistsAndOwnedByEffectiveGroupId")
        }
        ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead => {
            error::unimp("unary extended test predicate: FileExistsAndModifiedSinceLastRead")
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId => {
            error::unimp("unary extended test predicate: FileExistsAndOwnedByEffectiveUserId")
        }
        ast::UnaryPredicate::FileExistsAndIsSocket => {
            let path = Path::new(operand);
            Ok(try_get_file_type(path).map_or(false, |ft| ft.is_socket()))
        }
        ast::UnaryPredicate::ShellOptionEnabled => {
            error::unimp("unary extended test predicate: ShellOptionEnabled")
        }
        ast::UnaryPredicate::ShellVariableIsSetAndAssigned => Ok(shell.env.is_set(operand)),
        ast::UnaryPredicate::ShellVariableIsSetAndNameRef => {
            error::unimp("unary extended test predicate: ShellVariableIsSetAndNameRef")
        }
    }
}

fn try_get_file_type(path: &Path) -> Option<std::fs::FileType> {
    path.metadata().map(|metadata| metadata.file_type()).ok()
}

fn try_get_file_mode(path: &Path) -> Option<u32> {
    path.metadata().map(|metadata| metadata.mode()).ok()
}

fn apply_binary_predicate(
    op: &ast::BinaryPredicate,
    left: &str,
    right: &str,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::single_match_else)]
    match op {
        // N.B. The "=", "==", and "!=" operators don't compare 2 strings; they check
        // for whether the lefthand operand (a string) is matched by the righthand
        // operand (treated as a shell pattern).
        // TODO: implement case-insensitive matching if relevant via shopt options (nocasematch).
        ast::BinaryPredicate::StringMatchesPattern => {
            let s = left;
            let pattern = right;
            patterns::pattern_exactly_matches(pattern, s, shell.options.extended_globbing)
        }
        ast::BinaryPredicate::StringDoesNotMatchPattern => {
            let s = left;
            let pattern = right;
            let eq =
                patterns::pattern_exactly_matches(pattern, s, shell.options.extended_globbing)?;
            Ok(!eq)
        }
        ast::BinaryPredicate::StringMatchesRegex => {
            let s = left;
            let regex_pattern = right;
            let (matches, captures) =
                if let Some(captures) = patterns::regex_matches(regex_pattern, s)? {
                    (true, captures)
                } else {
                    (false, vec![])
                };

            let captures_value = variables::ShellValueLiteral::Array(ArrayLiteral(
                captures.into_iter().map(|c| (None, c)).collect(),
            ));

            shell.env.update_or_add(
                "BASH_REMATCH",
                captures_value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;

            Ok(matches)
        }
        ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers => {
            error::unimp("extended test binary predicate FilesReferToSameDeviceAndInodeNumbers")
        }
        ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot => {
            error::unimp("extended test binary predicate LeftFileIsNewerOrExistsWhenRightDoesNot")
        }
        ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes => error::unimp(
            "extended test binary predicate LeftFileIsOlderOrDoesNotExistWhenRightDoes",
        ),
        ast::BinaryPredicate::LeftSortsBeforeRight => {
            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left < right)
        }
        ast::BinaryPredicate::LeftSortsAfterRight => {
            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left > right)
        }
        ast::BinaryPredicate::ArithmeticEqualTo => Ok(apply_binary_arithmetic_predicate(
            left,
            right,
            |left, right| left == right,
        )),
        ast::BinaryPredicate::ArithmeticNotEqualTo => Ok(apply_binary_arithmetic_predicate(
            left,
            right,
            |left, right| left != right,
        )),
        ast::BinaryPredicate::ArithmeticLessThan => Ok(apply_binary_arithmetic_predicate(
            left,
            right,
            |left, right| left < right,
        )),
        ast::BinaryPredicate::ArithmeticLessThanOrEqualTo => Ok(apply_binary_arithmetic_predicate(
            left,
            right,
            |left, right| left <= right,
        )),
        ast::BinaryPredicate::ArithmeticGreaterThan => Ok(apply_binary_arithmetic_predicate(
            left,
            right,
            |left, right| left > right,
        )),
        ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo => Ok(
            apply_binary_arithmetic_predicate(left, right, |left, right| left >= right),
        ),
    }
}

fn apply_binary_arithmetic_predicate(left: &str, right: &str, op: fn(i64, i64) -> bool) -> bool {
    let left: Result<i64, _> = left.parse();
    let right: Result<i64, _> = right.parse();

    if let (Ok(left), Ok(right)) = (left, right) {
        op(left, right)
    } else {
        false
    }
}
