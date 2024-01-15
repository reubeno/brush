use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
};

use anyhow::Result;
use faccess::PathExt;
use parser::ast;

use crate::{expansion::expand_word, patterns, Shell};

#[async_recursion::async_recursion]
pub(crate) async fn eval_expression(
    expr: &ast::ExtendedTestExpr,
    shell: &mut Shell,
) -> Result<bool> {
    #[allow(clippy::single_match_else)]
    match expr {
        ast::ExtendedTestExpr::UnaryTest(op, operand) => {
            let expanded_operand = expand_word(shell, operand).await?;
            apply_unary_predicate(op, expanded_operand.as_str())
        }
        ast::ExtendedTestExpr::BinaryTest(op, left, right) => {
            let expanded_left = expand_word(shell, left).await?;
            let expanded_right = expand_word(shell, right).await?;
            apply_binary_predicate(op, expanded_left.as_str(), expanded_right.as_str())
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
            todo!("UNIMPLEMENTED: unary extended test predicate: FileExistsAndHasStickyBit")
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
            todo!("UNIMPLEMENTED: unary extended test predicate: FdIsOpenTerminal")
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
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId => todo!(
            "UNIMPLEMENTED: unary extended test predicate: FileExistsAndOwnedByEffectiveGroupId"
        ),
        ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead => todo!(
            "UNIMPLEMENTED: unary extended test predicate: FileExistsAndModifiedSinceLastRead"
        ),
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId => todo!(
            "UNIMPLEMENTED: unary extended test predicate: FileExistsAndOwnedByEffectiveUserId"
        ),
        ast::UnaryPredicate::FileExistsAndIsSocket => {
            let path = Path::new(operand);
            Ok(try_get_file_type(path).map_or(false, |ft| ft.is_socket()))
        }
        ast::UnaryPredicate::ShellOptionEnabled => {
            todo!("UNIMPLEMENTED: unary extended test predicate: ShellOptionEnabled")
        }
        ast::UnaryPredicate::ShellVariableIsSetAndAssigned => {
            todo!("UNIMPLEMENTED: unary extended test predicate: ShellVariableIsSetAndAssigned")
        }
        ast::UnaryPredicate::ShellVariableIsSetAndNameRef => {
            todo!("UNIMPLEMENTED: unary extended test predicate: ShellVariableIsSetAndNameRef")
        }
    }
}

fn try_get_file_type(path: &Path) -> Option<std::fs::FileType> {
    path.metadata().map(|metadata| metadata.file_type()).ok()
}

fn try_get_file_mode(path: &Path) -> Option<u32> {
    path.metadata().map(|metadata| metadata.mode()).ok()
}

fn apply_binary_predicate(op: &ast::BinaryPredicate, left: &str, right: &str) -> Result<bool> {
    #[allow(clippy::single_match_else)]
    match op {
        // N.B. The "=", "==", and "!=" operators don't compare 2 strings; they check
        // for whether the lefthand operand (a string) is matched by the righthand
        // operand (treated as a shell pattern).
        // TODO: implement case-insensitive matching if relevant via shopt options (nocasematch).
        ast::BinaryPredicate::StringMatchesPattern => {
            let s = left;
            let pattern = right;
            patterns::pattern_matches(pattern, s)
        }
        ast::BinaryPredicate::StringDoesNotMatchPattern => {
            let s = left;
            let pattern = right;
            let eq = patterns::pattern_matches(pattern, s)?;
            Ok(!eq)
        }
        ast::BinaryPredicate::StringMatchesRegex => {
            let s = left;
            let regex_pattern = right;
            patterns::regex_matches(regex_pattern, s)
        }
        ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers => todo!("UNIMPLEMENTED: extended test binary predicate FilesReferToSameDeviceAndInodeNumbers"),
        ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot => todo!("UNIMPLEMENTED: extended test binary predicate LeftFileIsNewerOrExistsWhenRightDoesNot"),
        ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes => todo!("UNIMPLEMENTED: extended test binary predicate LeftFileIsOlderOrDoesNotExistWhenRightDoes"),
        ast::BinaryPredicate::LeftSortsBeforeRight => {
            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left < right)
        },
        ast::BinaryPredicate::LeftSortsAfterRight => {
            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left > right)
        },
        ast::BinaryPredicate::ArithmeticEqualTo =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left == right)),
        ast::BinaryPredicate::ArithmeticNotEqualTo =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left != right)),
        ast::BinaryPredicate::ArithmeticLessThan =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left < right)),
        ast::BinaryPredicate::ArithmeticLessThanOrEqualTo =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left <= right)),
        ast::BinaryPredicate::ArithmeticGreaterThan =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left > right)),
        ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo =>
            Ok(apply_binary_arithmetic_predicate(left, right, |left, right| left >= right)),
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
