//! Tests for simple command parsing.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use anyhow::Result;

#[test]
fn parse_echo_hello() -> Result<()> {
    let input = "echo hello";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ls() -> Result<()> {
    let input = "ls";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_colon() -> Result<()> {
    let input = ":";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_true() -> Result<()> {
    let input = "true";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_echo_hello_world() -> Result<()> {
    let input = "echo hello world";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ls_la_home() -> Result<()> {
    let input = "ls -la /home";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_with_quoted_args() -> Result<()> {
    let input = r#"echo "hello world""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_with_single_quotes() -> Result<()> {
    let input = "echo 'hello world'";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_with_backslash_escape() -> Result<()> {
    let input = r"echo hello\ world";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_operator_followed_by_escaped_word() -> Result<()> {
    for input in [
        r"cat <\file",
        r"printf hi |\cat",
        r"false &&\echo ok",
        r"echo one;\echo two",
    ] {
        let _ = test_with_snapshot(input)?;
    }
    Ok(())
}

#[test]
fn parse_line_continuation_inside_operators() -> Result<()> {
    for input in [
        r"true &\
& echo yes",
        r"false |\
| echo no",
        r"echo x |\
& cat",
        r"case x in a) : ;\
; esac",
        r"cat <\
<EOF
body
EOF
",
    ] {
        let _ = test_with_snapshot(input)?;
    }
    Ok(())
}

#[test]
fn parse_command_with_variable() -> Result<()> {
    let input = "echo $HOME";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_with_command_substitution() -> Result<()> {
    let input = "echo $(whoami)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_with_backtick_substitution() -> Result<()> {
    let input = "echo `whoami`";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
