//! The numeric operand of `break`, `continue`, `exit`, `return` and `shift`.

use std::io::Write;

use brush_core::builtins::UsageError;
use brush_core::{ExecutionExitCode, ExecutionResult};

/// Parses a builtin's numeric operand: a decimal integer that fits in 64
/// bits, with an optional sign and optionally surrounded by blanks. Anything
/// else is a misuse of the builtin.
pub(crate) fn number(operand: &str) -> Result<i64, UsageError> {
    parse(operand).ok_or_else(|| UsageError::new(format!("{operand}: numeric argument required")))
}

/// Fails a builtin given more than its one operand. That isn't a misuse that
/// ends a POSIX shell: it's reported, and the builtin fails with the
/// invalid-usage status.
pub(crate) fn too_many(
    name: &str,
    mut stderr: impl Write,
) -> Result<ExecutionResult, brush_core::Error> {
    writeln!(stderr, "{name}: too many arguments")?;
    Ok(ExecutionExitCode::InvalidUsage.into())
}

/// Parses `operand` as a numeric operand, or returns `None` if it isn't one.
fn parse(operand: &str) -> Option<i64> {
    let number = operand
        .trim_start_matches(|c: char| c.is_ascii_whitespace())
        .trim_end_matches([' ', '\t']);
    let digits = number.strip_prefix(['+', '-']).unwrap_or(number);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_signed_decimal_numbers_with_blanks_around() {
        assert_eq!(parse("3"), Some(3));
        assert_eq!(parse(" 3"), Some(3));
        assert_eq!(parse("3 \t"), Some(3));
        assert_eq!(parse("\n+3"), Some(3));
        assert_eq!(parse("-3"), Some(-3));
        assert_eq!(parse("99999999999"), Some(99_999_999_999));
        assert_eq!(parse("-9223372036854775808"), Some(i64::MIN));
    }

    #[test]
    fn rejects_anything_else() {
        for operand in [
            "",
            " ",
            "+",
            "-",
            "abc",
            "3x",
            "0x10",
            "1 2",
            "3\n",
            "--3",
            "9223372036854775808",
        ] {
            assert_eq!(parse(operand), None, "{operand:?}");
        }
    }
}
