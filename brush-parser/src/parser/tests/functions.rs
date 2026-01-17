//! Tests for function definition parsing.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use anyhow::Result;

#[test]
fn parse_function_basic() -> Result<()> {
    let input = "foo() { echo hello; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_keyword() -> Result<()> {
    let input = "function foo { echo hello; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_keyword_with_parens() -> Result<()> {
    let input = "function foo() { echo hello; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_redirect() -> Result<()> {
    let input = "foo() { echo 1; } 2>&1 | cat";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_stderr_redirect() -> Result<()> {
    let input = "foo() { echo 1; } |& cat";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_multiline() -> Result<()> {
    let input = r"foo() {
    echo hello
    echo world
}";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_subshell_body() -> Result<()> {
    let input = "foo() ( echo subshell )";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_local_vars() -> Result<()> {
    let input = r"foo() {
    local x=1
    echo $x
}";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_hyphenated_name() -> Result<()> {
    let input = "debug-print() { :; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_dotted_name() -> Result<()> {
    let input = "dolib.so() { :; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_keyword_hyphenated_name() -> Result<()> {
    let input = "function debug-print-function { :; }";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_escaped_quotes_in_body() -> Result<()> {
    let input = r#"myfunc() { eval "foo() { bar \"hello\"; }"; }"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_eval_escaped_dollar_at() -> Result<()> {
    let input = r#"EXPORT_FUNCTIONS() {
    local __phase
    for __phase in "$@"; do
        eval "${__phase}() { ${ECLASS}_${__phase} \"\$@\"; }"
    done
}"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
