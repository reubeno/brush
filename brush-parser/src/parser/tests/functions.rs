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
fn parse_function_with_extended_test_body() -> Result<()> {
    // bash accepts `[[ ... ]]` (itself a compound command) as a brace-less function
    // body, e.g. real-world usage in Gentoo Portage's bin/eapi.sh:
    //   ___eapi_has_pkg_pretend() [[ ${1-${EAPI-0}} != [0-3] ]]
    let input = "foo() [[ -n $1 ]]";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_extended_test_body_and_redirect() -> Result<()> {
    let input = "foo() [[ -n $1 ]] 2>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
