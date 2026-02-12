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
fn parse_here_doc_in_double_quoted_command_substitution() -> Result<()> {
    let input = "test1=\"$(cat <<EOF\nsomething\nEOF\n)\"\necho \"${test1}\"\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Verify that partial double-quoted heredoc input is rejected (not silently
/// truncated), so the shell accumulation loop keeps reading more lines.
#[test]
fn parse_here_doc_in_double_quoted_command_substitution_partial_must_fail() {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    let partial = "test1=\"$(cat <<EOF\n";
    let configs = [
        ParserConfig {
            name: "peg",
            parser_impl: ParserImpl::Peg,
        },
        #[cfg(feature = "winnow-parser")]
        ParserConfig {
            name: "winnow",
            parser_impl: ParserImpl::Winnow,
        },
    ];
    for config in &configs {
        let result = parse_with_config(partial, config);
        assert!(
            result.is_err(),
            "{} parser should reject partial input {:?}, but got: {:?}",
            config.name,
            partial,
            result
        );
    }
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

// The following tests exercise heredoc bodies containing `)` inside `$()`
// command substitutions.  Both parsers should handle this correctly.

#[test]
fn parse_here_doc_with_parens_in_command_substitution_peg() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    let input = r#"X=$(cat <<EOF
print(foo())
EOF
)
echo "$X"
"#;
    let config = ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("PEG parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[cfg(feature = "winnow-parser")]
#[test]
fn parse_here_doc_with_parens_in_command_substitution_winnow() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    let input = r#"X=$(cat <<EOF
print(foo())
EOF
)
echo "$X"
"#;
    let config = ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("Winnow parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_tab_stripped_with_parens_in_command_substitution_peg() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    let input = "X=$(\n\tcat <<-EOF\n\t\tprint(foo())\n\tEOF\n)\necho \"$X\"\n";
    let config = ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("PEG parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[cfg(feature = "winnow-parser")]
#[test]
fn parse_here_doc_tab_stripped_with_parens_in_command_substitution_winnow() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    let input = "X=$(\n\tcat <<-EOF\n\t\tprint(foo())\n\tEOF\n)\necho \"$X\"\n";
    let config = ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("Winnow parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_doc_in_command_substitution_eclass_pattern_peg() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    // Reduced from gentoo python-utils-r1.eclass: heredoc with ) inside $()
    let input = r#"PYTHON_STDLIB=$(
    "${PYTHON}" - "${EPREFIX}/usr" <<-EOF || die
		import sys, sysconfig
		print(sysconfig.get_path("stdlib", vars={"installed_base": sys.argv[1]}))
	EOF
)
"#;
    let config = ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("PEG parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[cfg(feature = "winnow-parser")]
#[test]
fn parse_here_doc_in_command_substitution_eclass_pattern_winnow() -> Result<()> {
    use super::{ParserConfig, parse_with_config};
    use crate::parser::ParserImpl;

    // Reduced from gentoo python-utils-r1.eclass: heredoc with ) inside $()
    let input = r#"PYTHON_STDLIB=$(
    "${PYTHON}" - "${EPREFIX}/usr" <<-EOF || die
		import sys, sysconfig
		print(sysconfig.get_path("stdlib", vars={"installed_base": sys.argv[1]}))
	EOF
)
"#;
    let config = ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };
    let result = parse_with_config(input, &config)
        .map_err(|e| anyhow::anyhow!("Winnow parser failed: {e}\nInput: {input}"))?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
