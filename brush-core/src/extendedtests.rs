use brush_parser::ast;
use std::path::Path;

use crate::{
    env, error, expansion, namedoptions, patterns,
    sys::{fs::MetadataExt, fs::PathExt, users},
    variables::{self, ArrayLiteral},
    Shell,
};

#[async_recursion::async_recursion]
pub(crate) async fn eval_extended_test_expr(
    expr: &ast::ExtendedTestExpr,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::single_match_else)]
    match expr {
        ast::ExtendedTestExpr::UnaryTest(op, operand) => {
            apply_unary_predicate(op, operand, shell).await
        }
        ast::ExtendedTestExpr::BinaryTest(op, left, right) => {
            apply_binary_predicate(op, left, right, shell).await
        }
        ast::ExtendedTestExpr::And(left, right) => {
            let result = eval_extended_test_expr(left, shell).await?
                && eval_extended_test_expr(right, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Or(left, right) => {
            let result = eval_extended_test_expr(left, shell).await?
                || eval_extended_test_expr(right, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Not(expr) => {
            let result = !eval_extended_test_expr(expr, shell).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Parenthesized(expr) => eval_extended_test_expr(expr, shell).await,
    }
}

async fn apply_unary_predicate(
    op: &ast::UnaryPredicate,
    operand: &ast::Word,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    let expanded_operand = expansion::basic_expand_word(shell, operand).await?;

    if shell.options.print_commands_and_arguments {
        shell.trace_command(std::format!("[[ {op} {expanded_operand} ]]"))?;
    }

    apply_unary_predicate_to_str(op, expanded_operand.as_str(), shell)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn apply_unary_predicate_to_str(
    op: &ast::UnaryPredicate,
    operand: &str,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::match_single_binding)]
    match op {
        ast::UnaryPredicate::StringHasNonZeroLength => Ok(!operand.is_empty()),
        ast::UnaryPredicate::StringHasZeroLength => Ok(operand.is_empty()),
        ast::UnaryPredicate::FileExists => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists())
        }
        ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_block_device())
        }
        ast::UnaryPredicate::FileExistsAndIsCharSpecialFile => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_char_device())
        }
        ast::UnaryPredicate::FileExistsAndIsDir => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.is_dir())
        }
        ast::UnaryPredicate::FileExistsAndIsRegularFile => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.is_file())
        }
        ast::UnaryPredicate::FileExistsAndIsSetgid => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_setgid())
        }
        ast::UnaryPredicate::FileExistsAndIsSymlink => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.is_symlink())
        }
        ast::UnaryPredicate::FileExistsAndHasStickyBit => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_sticky_bit())
        }
        ast::UnaryPredicate::FileExistsAndIsFifo => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_fifo())
        }
        ast::UnaryPredicate::FileExistsAndIsReadable => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.readable())
        }
        ast::UnaryPredicate::FileExistsAndIsNotZeroLength => {
            let path = shell.get_absolute_path(Path::new(operand));
            if let Ok(metadata) = path.metadata() {
                Ok(metadata.len() > 0)
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::FdIsOpenTerminal => {
            if let Ok(fd) = operand.parse::<u32>() {
                if let Some(open_file) = shell.open_files.files.get(&fd) {
                    Ok(open_file.is_term())
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::FileExistsAndIsSetuid => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_setuid())
        }
        ast::UnaryPredicate::FileExistsAndIsWritable => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.writable())
        }
        ast::UnaryPredicate::FileExistsAndIsExecutable => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.executable())
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId => {
            let path = shell.get_absolute_path(Path::new(operand));
            if !path.exists() {
                return Ok(false);
            }

            let md = path.metadata()?;
            Ok(md.gid() == users::get_effective_gid()?)
        }
        ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead => {
            error::unimp("unary extended test predicate: FileExistsAndModifiedSinceLastRead")
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId => {
            let path = shell.get_absolute_path(Path::new(operand));
            if !path.exists() {
                return Ok(false);
            }

            let md = path.metadata()?;
            Ok(md.uid() == users::get_effective_uid()?)
        }
        ast::UnaryPredicate::FileExistsAndIsSocket => {
            let path = shell.get_absolute_path(Path::new(operand));
            Ok(path.exists_and_is_socket())
        }
        ast::UnaryPredicate::ShellOptionEnabled => {
            let shopt_name = operand;
            if let Some(option) = namedoptions::SET_O_OPTIONS.get(shopt_name) {
                Ok((option.getter)(&shell.options))
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::ShellVariableIsSetAndAssigned => Ok(shell.env.is_set(operand)),
        ast::UnaryPredicate::ShellVariableIsSetAndNameRef => {
            error::unimp("unary extended test predicate: ShellVariableIsSetAndNameRef")
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn apply_binary_predicate(
    op: &ast::BinaryPredicate,
    left: &ast::Word,
    right: &ast::Word,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    #[allow(clippy::single_match_else)]
    match op {
        ast::BinaryPredicate::StringMatchesRegex => {
            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            let s = expansion::basic_expand_word(shell, left).await?;
            let regex = expansion::basic_expand_regex(shell, right).await?;

            let (matches, captures) = if let Some(captures) = regex.matches(s.as_str())? {
                (true, captures)
            } else {
                (false, vec![])
            };

            let captures_value = variables::ShellValueLiteral::Array(ArrayLiteral(
                captures
                    .into_iter()
                    .map(|c| (None, c.unwrap_or_default()))
                    .collect(),
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
        ast::BinaryPredicate::StringContainsSubstring => {
            let s = expansion::basic_expand_word(shell, left).await?;
            let substring = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {s} {op} {substring} ]]"))?;
            }

            Ok(s.contains(substring.as_str()))
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
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left < right)
        }
        ast::BinaryPredicate::LeftSortsAfterRight => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            // TODO: According to docs, should be lexicographical order of the current locale.
            Ok(left > right)
        }
        ast::BinaryPredicate::ArithmeticEqualTo => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left == right,
            ))
        }
        ast::BinaryPredicate::ArithmeticNotEqualTo => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left != right,
            ))
        }
        ast::BinaryPredicate::ArithmeticLessThan => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left < right,
            ))
        }
        ast::BinaryPredicate::ArithmeticLessThanOrEqualTo => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left <= right,
            ))
        }
        ast::BinaryPredicate::ArithmeticGreaterThan => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left > right,
            ))
        }
        ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo => {
            let left = expansion::basic_expand_word(shell, left).await?;
            let right = expansion::basic_expand_word(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                shell.trace_command(std::format!("[[ {left} {op} {right} ]]"))?;
            }

            Ok(apply_binary_arithmetic_predicate(
                left.as_str(),
                right.as_str(),
                |left, right| left >= right,
            ))
        }
        // N.B. The "=", "==", and "!=" operators don't compare 2 strings; they check
        // for whether the lefthand operand (a string) is matched by the righthand
        // operand (treated as a shell pattern).
        // TODO: implement case-insensitive matching if relevant via shopt options (nocasematch).
        ast::BinaryPredicate::StringExactlyMatchesPattern => {
            let s = expansion::basic_expand_word(shell, left).await?;
            let pattern = expansion::basic_expand_pattern(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                let expanded_right = expansion::basic_expand_word(shell, right).await?;
                shell.trace_command(std::format!("[[ {s} {op} {expanded_right} ]]"))?;
            }

            pattern.exactly_matches(s.as_str(), shell.options.extended_globbing)
        }
        ast::BinaryPredicate::StringDoesNotExactlyMatchPattern => {
            let s = expansion::basic_expand_word(shell, left).await?;
            let pattern = expansion::basic_expand_pattern(shell, right).await?;

            if shell.options.print_commands_and_arguments {
                let expanded_right = expansion::basic_expand_word(shell, right).await?;
                shell.trace_command(std::format!("[[ {s} {op} {expanded_right} ]]"))?;
            }

            let eq = pattern.exactly_matches(s.as_str(), shell.options.extended_globbing)?;
            Ok(!eq)
        }
    }
}

pub(crate) fn apply_binary_predicate_to_strs(
    op: &ast::BinaryPredicate,
    left: &str,
    right: &str,
    shell: &mut Shell,
) -> Result<bool, error::Error> {
    match op {
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
        ast::BinaryPredicate::StringExactlyMatchesPattern => {
            let pattern = patterns::Pattern::from(right);
            pattern.exactly_matches(left, shell.options.extended_globbing)
        }
        ast::BinaryPredicate::StringDoesNotExactlyMatchPattern => {
            let pattern = patterns::Pattern::from(right);
            let eq = pattern.exactly_matches(left, shell.options.extended_globbing)?;
            Ok(!eq)
        }
        _ => error::unimp("unsupported test binary predicate"),
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
