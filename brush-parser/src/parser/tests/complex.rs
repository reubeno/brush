//! Complex and integration tests that combine multiple parser features.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use anyhow::Result;

#[test]
fn parse_shebang_and_program() -> Result<()> {
    let input = r#"#!/usr/bin/env bash

for f in A B C; do

    # sdfsdf
    echo "${f@L}" >&2

   done
"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_case_with_newlines() -> Result<()> {
    let input = r"case x in
x)
    echo y;;
esac";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_case_no_semicolon() -> Result<()> {
    let input = r"case x in
x)
    echo y
esac";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_nested_if_while() -> Result<()> {
    let input = r#"if true; then
    while read line; do
        echo "$line"
    done < file.txt
fi"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_if() -> Result<()> {
    let input = r#"myfunc() {
    if [[ -z "$1" ]]; then
        echo "No argument"
        return 1
    fi
    echo "Got: $1"
}"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_pipeline_with_redirections() -> Result<()> {
    let input = "cat < input.txt | grep pattern | tee output.txt > /dev/null 2>&1";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_command_substitution_nested() -> Result<()> {
    let input = "echo \"$(echo $(pwd))\"";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_for_with_command_substitution() -> Result<()> {
    let input = "for f in $(ls *.txt); do cat \"$f\"; done";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_arithmetic_in_condition() -> Result<()> {
    let input = "if (( x > 5 )); then echo big; else echo small; fi";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_brace_expansion_context() -> Result<()> {
    let input = "echo {a,b,c}";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
#[allow(clippy::literal_string_with_formatting_args)]
fn parse_parameter_expansion_complex() -> Result<()> {
    let input = r#"echo "${var:-default}" "${var:+alt}" "${var:=assign}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_subshell_with_assignments() -> Result<()> {
    let input = "( x=1; y=2; echo $((x + y)) )";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_coprocess() -> Result<()> {
    let input = "coproc cat";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_multiple_here_docs() -> Result<()> {
    let input = r"cmd <<EOF1 <<EOF2
first
EOF1
second
EOF2
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_backgrounded_commands() -> Result<()> {
    let input = "cmd1 & cmd2 & cmd3";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_mixed_sequences() -> Result<()> {
    let input = "cmd1; cmd2 && cmd3 || cmd4; cmd5";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_complex_array_operations() -> Result<()> {
    let input = r#"arr=($(seq 1 10)); echo "${arr[@]}"; echo "${#arr[@]}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_glob_patterns() -> Result<()> {
    let input = "ls *.txt **/foo.* file?.txt";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_extglob_patterns() -> Result<()> {
    let input = "ls !(*.txt) +(foo|bar) ?(a|b)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_tilde_expansion() -> Result<()> {
    let input = "cd ~/projects; ls ~user/home";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Winnow parser must handle empty and comment-only inputs.
#[test]
fn parse_empty_program() -> Result<()> {
    let result = test_with_snapshot("")?;
    assert_snapshot_redacted!(ParseResult {
        input: "",
        result: &result
    });
    Ok(())
}

#[test]
fn parse_comment_only() -> Result<()> {
    let input = "# hello\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_comment_no_trailing_newline() -> Result<()> {
    let input = "# hello";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_comments_then_command() -> Result<()> {
    let input = "# comment\n# another\necho hi\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Array assignment: VAR=( elem1 elem2 )
#[test]
fn parse_array_assignment() -> Result<()> {
    let input = "ALL_LLVM_TARGETS=( AArch64 AMDGPU ARM )";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Multi-line array assignment
#[test]
fn parse_array_assignment_multiline() -> Result<()> {
    let input = "ALL_LLVM_TARGETS=( AArch64 AMDGPU ARC ARM AVR BPF\n\tLoongArch M68k Mips X86 )";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Parse all eclasses in the portage-repo/gentoo tree with the winnow parser.
/// This is a broad integration test; skip gracefully if the tree isn't present.
#[test]
#[cfg(feature = "winnow-parser")]
fn winnow_parse_all_eclasses() -> Result<()> {
    // Track expected failures: the eof check in program() correctly rejects
    // eclasses that use constructs the winnow parser doesn't yet support.
    // This count should decrease as the parser improves.
    const EXPECTED_FAILURES: usize = 62;

    use super::parse_with_config;
    use crate::parser::ParserImpl;

    let eclass_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("../portage-repo/gentoo/eclass");

    if !eclass_dir.is_dir() {
        eprintln!("Skipping: eclass dir not found at {}", eclass_dir.display());
        return Ok(());
    }

    let winnow_cfg = super::ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };

    let mut failures = Vec::new();
    let mut total = 0;

    for entry in std::fs::read_dir(&eclass_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "eclass") {
            total += 1;
            let content = std::fs::read_to_string(&path)?;
            if let Err(e) = parse_with_config(&content, &winnow_cfg) {
                failures.push((
                    path.file_name().unwrap().to_string_lossy().to_string(),
                    format!("{e}"),
                ));
            }
        }
    }

    failures.sort();
    let failure_count = failures.len();

    if failure_count > 0 {
        eprintln!("\n{failure_count}/{total} eclasses failed to parse:");
        for (name, err) in &failures {
            eprintln!("  {name}: {err}");
        }
    }

    if failure_count > EXPECTED_FAILURES {
        return Err(anyhow::anyhow!(
            "Regression: {failure_count}/{total} eclasses failed (expected at most {EXPECTED_FAILURES})"
        ));
    }
    if failure_count < EXPECTED_FAILURES {
        eprintln!(
            "Progress! Only {failure_count}/{total} eclasses failed (expected {EXPECTED_FAILURES}). \
             Please update EXPECTED_FAILURES."
        );
    }

    eprintln!(
        "{}/{total} eclasses parsed OK with winnow",
        total - failure_count,
    );
    Ok(())
}

/// Parse the rust ebuild itself.
#[test]
#[cfg(feature = "winnow-parser")]
fn winnow_parse_rust_ebuild() -> Result<()> {
    use super::parse_with_config;
    use crate::parser::ParserImpl;

    let ebuild_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("../portage-repo/gentoo/dev-lang/rust/rust-1.88.0.ebuild");

    if !ebuild_path.exists() {
        eprintln!("Skipping: ebuild not found at {}", ebuild_path.display());
        return Ok(());
    }

    let winnow_cfg = super::ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };

    let content = std::fs::read_to_string(&ebuild_path)?;
    parse_with_config(&content, &winnow_cfg)
        .map_err(|e| anyhow::anyhow!("Failed to parse rust ebuild: {e}"))?;

    eprintln!("rust-1.88.0.ebuild parsed OK");
    Ok(())
}

/// Two function definitions inside an if-then block
#[test]
fn parse_functions_inside_if() -> Result<()> {
    let input = r"if [[ -z ${FOO} ]]; then
FOO=1
myfunc1() {
    echo a
}
myfunc2() {
    local x
    if [[ $x -ge 5 ]]; then
        echo yes
    fi
}
fi
";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_eclass_like_structure() -> Result<()> {
    let input = r#"if [[ -z ${_FOO_ECLASS} ]]; then
_FOO_ECLASS=1
case ${EAPI} in
	7|8) ;;
	*) die "unsupported" ;;
esac
_ALL_IMPLS=(
	impl1
	impl2_{3..5}
)
readonly _ALL_IMPLS
_HIST_IMPLS=(
	old1
	old2_{8,9}
)
readonly _HIST_IMPLS
_verify() {
	local impl pattern
	for pattern; do
		case ${pattern} in
			-[23])
				continue
				;;
		esac
	done
}
_set_impls() {
	local i
	if [[ ${BASH_VERSINFO[0]} -ge 5 ]]; then
		[[ ${COMPAT@a} == *a* ]]
	else
		[[ $(declare -p COMPAT) == "declare -a"* ]]
	fi
	if [[ ${?} -ne 0 ]]; then
		die 'bad'
	fi
}
fi
"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// if-else with extended tests containing command substitution
#[test]
fn parse_if_else_ext_test() -> Result<()> {
    let input = r#"if [[ ${BASH_VERSINFO[0]} -ge 5 ]]; then
	[[ ${FOO@a} == *a* ]]
else
	[[ $(declare -p FOO) == "declare -a"* ]]
fi
"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Parameter transformation ${var@a}
#[test]
fn parse_parameter_transformation() -> Result<()> {
    let input = "[[ ${PYTHON_COMPAT@a} == *a* ]]";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Parameter transformation ${var@a} inside function
#[test]
fn parse_param_transform_in_function() -> Result<()> {
    let input = "myfunc() {\n\tif [[ ${PYTHON_COMPAT@a} == *a* ]]; then\n\t\techo yes\n\tfi\n}\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Array with brace expansion containing commas
#[test]
fn parse_array_with_comma_brace_expansion() -> Result<()> {
    let input = "_PYTHON_HISTORICAL_IMPLS=(\n\tjython2_7\n\tpypy pypy1_{8,9} pypy2_0 pypy3\n\tpython2_{5..7}\n\tpython3_{1..10}\n)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Standalone heredoc followed by another command
#[test]
fn parse_heredoc_then_command() -> Result<()> {
    let input = "cat <<EOF\nhello\nEOF\necho done\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Heredoc with <<- (tab stripping) followed by another command
#[test]
fn parse_heredoc_dash_then_command() -> Result<()> {
    let input = "cat <<- EOF\n\thello\n\tEOF\necho done\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Heredoc with <<- inside a function definition
#[test]
fn parse_heredoc_dash_in_function() -> Result<()> {
    let input = "myfunc() {\n\tcat <<- EOF\n\t\thello\n\tEOF\n}\necho done\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Heredoc inside a command substitution — used in eclasses like:
///   RESULT=$(cmd <<-EOF
///       content
///   EOF
///   )
#[test]
fn parse_heredoc_in_command_substitution() -> Result<()> {
    let input = r"RESULT=$(
    cat <<EOF
hello world
EOF
)";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Heredoc inside $() with tab-stripped delimiter (<<-)
/// NOTE: winnow parser does not yet strip leading tabs for <<- heredocs inside
/// command substitutions. This test only exercises the PEG parser for now.
#[test]
fn parse_heredoc_dash_in_command_substitution() -> Result<()> {
    use super::parse_with_config;
    use crate::parser::ParserImpl;
    let input = "RESULT=$(\n\tcat <<-EOF\n\t\thello world\n\tEOF\n)";
    let peg_cfg = super::ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    };
    let result = parse_with_config(input, &peg_cfg)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

/// Regression test: parse python-utils-r1.eclass and verify the outer
/// if-guard body is non-empty.  This eclass wraps its entire body in:
///   if [[ -z ${`_PYTHON_UTILS_R1_ECLASS`} ]]; then ... fi
/// The winnow parser was previously producing an AST where the then-body
/// was empty, causing all definitions inside to be lost at runtime.
/// Root causes: (1) `ext_test_word` couldn't parse multi-segment words like
/// "declare -a"*, (2) `io_redirect` couldn't parse process substitution
/// targets like < <(cmd).
#[test]
#[cfg(feature = "winnow-parser")]
fn winnow_parse_python_utils_eclass_if_guard() -> Result<()> {
    use super::parse_with_config;
    use crate::ast::{Command, CompoundCommand};
    use crate::parser::ParserImpl;

    let eclass_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("../portage-repo/gentoo/eclass/python-utils-r1.eclass");

    if !eclass_path.exists() {
        eprintln!("Skipping: eclass not found at {}", eclass_path.display());
        return Ok(());
    }

    let winnow_cfg = super::ParserConfig {
        name: "winnow",
        parser_impl: ParserImpl::Winnow,
    };

    let content = std::fs::read_to_string(&eclass_path)?;
    let program = parse_with_config(&content, &winnow_cfg)
        .map_err(|e| anyhow::anyhow!("Failed to parse python-utils-r1.eclass: {e}"))?;

    if program.complete_commands.len() != 1 {
        return Err(anyhow::anyhow!(
            "Expected 1 top-level command, got {}",
            program.complete_commands.len()
        ));
    }

    // The first (and only) top-level command should be the if-guard
    let first_cmd = &program.complete_commands[0].0[0].0.first.seq[0];
    let Command::Compound(CompoundCommand::IfClause(if_cmd), _) = first_cmd else {
        return Err(anyhow::anyhow!("Expected top-level IfClause"));
    };
    if if_cmd.then.0.is_empty() {
        return Err(anyhow::anyhow!(
            "The outer if-guard then-body is EMPTY — regression!"
        ));
    }

    Ok(())
}

#[test]
fn parse_multiline_array_with_brace_expansion() -> Result<()> {
    let input = "_PYTHON_ALL_IMPLS=(\n\tpypy3_11\n\tpython3_{13..14}t\n\tpython3_{11..14}\n)\n";
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
