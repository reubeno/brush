//! Tests for here-document parsing.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use anyhow::Result;

#[test]
fn parse_here_doc_basic() -> Result<()> {
    let input = r"cat <<EOF
content line 1
content line 2
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_no_trailing_newline() -> Result<()> {
    let input = r"cat <<EOF
Something
EOF";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_tab_removal() -> Result<()> {
    let input = "cat <<-EOF\n\tcontent with tab\nEOF\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_quoted_delimiter() -> Result<()> {
    let input = r"cat <<'EOF'
$variable should not expand
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_double_quoted_delimiter() -> Result<()> {
    let input = r#"cat <<"EOF"
$variable should not expand
EOF
"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_with_expansion() -> Result<()> {
    let input = r"cat <<EOF
Hello $USER
Your home is $HOME
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_empty() -> Result<()> {
    let input = r"cat <<EOF
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_with_command_after() -> Result<()> {
    let input = r"cat <<EOF | grep hello
hello world
goodbye world
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_with_fd() -> Result<()> {
    let input = r"command 3<<EOF
content for fd 3
EOF
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_pathological_here_docs() {
    // Expansions left open around a here-document, including one nested in another. Each must
    // be reported as an error.
    let peg = super::ParserConfig {
        name: "peg",
        parser_impl: crate::parser::ParserImpl::Peg,
    };
    let results: Vec<_> = [
        "$(cat <<EOF ${y}\nEOF",
        "echo $(echo $(cat <<EOF) x\nEOF",
        "echo $(echo $(cat <<EOF)\nbody\nEOF\n",
    ]
    .into_iter()
    .map(|input| {
        let result = super::parse_with_config(input, &peg);
        (input, result.map(|_| ()).map_err(|e| e.to_string()))
    })
    .collect();
    insta::assert_ron_snapshot!(results);
}

#[test]
fn parse_here_doc_left_open_in_nested_command_substitution() -> Result<()> {
    // The inner substitution ends before the here-document's body, which follows the line;
    // bash accepts this (with a warning). The body belongs in the inner substitution's text,
    // with its operator and tag, and the outer substitution must still end at its own
    // closing paren, after the body and the command that follows it.
    let input = "echo $(echo $(cat <<EOF)\nbody\nEOF\necho more\n)\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
