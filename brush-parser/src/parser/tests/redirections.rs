//! Tests for redirection parsing.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use crate::ast::{AssignmentName, Command, CommandPrefixOrSuffixItem, IoFdSpec, IoRedirect};
use crate::parser::{Parser, ParserOptions};
use anyhow::Result;

// File redirections

#[test]
fn parse_redirect_output() -> Result<()> {
    let input = "echo hello > file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_input() -> Result<()> {
    let input = "cat < input.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_append() -> Result<()> {
    let input = "echo hello >> file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_clobber() -> Result<()> {
    let input = "echo hello >| file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_read_write() -> Result<()> {
    let input = "cat <> file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// FD operations

#[test]
fn parse_redirect_stderr_to_stdout() -> Result<()> {
    let input = "command 2>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_stdout_to_stderr() -> Result<()> {
    let input = "command 1>&2";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_fd_close() -> Result<()> {
    let input = "command 2>&-";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_stdin_dup() -> Result<()> {
    let input = "command <&3";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Combined redirections

#[test]
fn parse_redirect_output_and_error() -> Result<()> {
    let input = "command &> file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_output_and_error_append() -> Result<()> {
    let input = "command &>> file.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_multiple() -> Result<()> {
    let input = "command < input.txt > output.txt 2>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Process substitution

#[test]
fn parse_process_substitution_read() -> Result<()> {
    let input = "diff <(sort file1) <(sort file2)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_process_substitution_write() -> Result<()> {
    let input = "tee >(grep error > errors.txt)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Here string

#[test]
fn parse_here_string() -> Result<()> {
    let input = "cat <<< 'hello world'";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_here_string_with_variable() -> Result<()> {
    let input = "cat <<< $variable";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Out-of-range I/O numbers

#[test]
fn parse_io_number_out_of_range_does_not_panic() -> Result<()> {
    // Regression: the `io_number` rule unwrapped the integer parse, so a run of
    // digits too large for an `IoFd` panicked the parser rather than declining
    // the rule. The digits are reinterpreted as an ordinary word instead.
    let input = "echo 99999999999999999999>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_io_number_in_range_still_recognized() -> Result<()> {
    let input = "echo 2>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// `{name}` in place of an I/O number

#[test]
fn parse_redirect_variable() -> Result<()> {
    let input = "echo hi {fd}>out {a[i+1]}<&0 {h}<<<str";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_redirect_variable_on_compound_command() -> Result<()> {
    let input = "{ :; } {fd}>out";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Returns the file descriptors given for the redirections of the first command in `input`.
fn redirect_fds(input: &str, sh_mode: bool) -> Result<Vec<Option<IoFdSpec>>> {
    let options = ParserOptions {
        sh_mode,
        ..Default::default()
    };
    let program = Parser::new(std::io::Cursor::new(input), &options).parse_program()?;
    let redirects: Vec<&IoRedirect> = match &program.complete_commands[0].0[0].0.first.seq[0] {
        Command::Simple(c) => (c.prefix.iter().flat_map(|p| &p.0))
            .chain(c.suffix.iter().flat_map(|s| &s.0))
            .filter_map(|item| match item {
                CommandPrefixOrSuffixItem::IoRedirect(r) => Some(r),
                _ => None,
            })
            .collect(),
        Command::Compound(_, redirects) => redirects.iter().flat_map(|l| &l.0).collect(),
        Command::Function(_) => vec![],
    };
    Ok(redirects
        .into_iter()
        .map(|r| match r {
            IoRedirect::File(fd, ..)
            | IoRedirect::HereDocument(fd, _)
            | IoRedirect::HereString(fd, _) => fd.clone(),
            IoRedirect::OutputAndError(..) => None,
        })
        .collect())
}

#[test]
#[expect(clippy::panic_in_result_fn)]
fn parse_redirect_variable_forms() -> Result<()> {
    let var = |name: &str| {
        Some(IoFdSpec::Variable(AssignmentName::VariableName(
            name.into(),
        )))
    };
    let elem = |name: &str, index: &str| {
        Some(IoFdSpec::Variable(AssignmentName::ArrayElementName(
            name.into(),
            index.into(),
        )))
    };

    for (input, expected) in [
        ("exec {fd}>&-", vec![var("fd")]),
        ("exec {fd}<&-", vec![var("fd")]),
        ("cat {fd}<<EOF\nx\nEOF\n", vec![var("fd")]),
        ("cat {fd}<<-EOF\nx\nEOF\n", vec![var("fd")]),
        ("exec {_x9}>|f {FD_2}<f", vec![var("_x9"), var("FD_2")]),
        (
            r#"exec {a[$i]}>>f {m["k"]}<>f {b[c[1]]}>f"#,
            vec![elem("a", "$i"), elem("m", r#""k""#), elem("b", "c[1]")],
        ),
        (
            "exec 3>f {fd}>&3-",
            vec![Some(IoFdSpec::Number(3)), var("fd")],
        ),
        ("{fd}>f echo", vec![var("fd")]),
        ("while :; do :; done {fd}<f", vec![var("fd")]),
    ] {
        assert_eq!(redirect_fds(input, false)?, expected, "input: {input:?}");
    }

    Ok(())
}

#[test]
#[expect(clippy::panic_in_result_fn)]
fn parse_words_that_only_look_like_redirect_variables() -> Result<()> {
    for input in [
        "echo {fd} >f",
        r#"echo "{fd}">f"#,
        r"echo \{fd}>f",
        "echo x{fd}>f",
        "echo {fd}x>f",
        "echo {1x}>f",
        "echo {}>f",
        "echo {a-b}>f",
        "echo {$v}>f",
        "echo {a[]}>f",
        "echo {a[1]]}>f",
        "echo {fd}&>f",
        "echo {a,b}>f",
    ] {
        let fds = redirect_fds(input, false)?;
        assert!(
            !fds.iter()
                .any(|fd| matches!(fd, Some(IoFdSpec::Variable(_)))),
            "input: {input:?}, fds: {fds:?}"
        );
    }

    // `{name}` redirections are an extension to sh.
    assert_eq!(redirect_fds("echo {fd}>f", true)?, vec![None]);

    Ok(())
}
