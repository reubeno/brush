//! Unit tests for winnow parser specific issues.
//!
//! These tests target the specific cases that currently fail with the winnow parser
//! but work with the PEG parser. They are designed to help debug and fix
//! winnow parser implementation issues.

use super::{ParseResult, test_with_snapshot};
use crate::assert_snapshot_redacted;
use anyhow::Result;

// Array indexing tests

#[test]
fn parse_array_index_assignment_with_variable_expansion() -> Result<()> {
    let input = r#"x=(3 2 1)
y[${x[0]}]=10
y[x[1]]=11
declare -p y"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_array_index_with_unquoted_variable() -> Result<()> {
    let input = r#"x=(3 2 1)
y[x[0]]=10
declare -p y"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// ANSI-C quoting tests

#[test]
fn parse_ansi_c_quotes_newline() -> Result<()> {
    let input = r#"single_quoted='\n'
echo "Single quoted len: ${#single_quoted}"
ansi_c_quoted=$'\n'
echo "ANSI-C quoted len: ${#ansi_c_quoted}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ansi_c_quotes_hex_escape() -> Result<()> {
    let input = r#"echo -n "0.  "$'\x' | hexdump -C
echo -n "1.  "$'\x65' | hexdump -C"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ansi_c_quotes_braced_hex() -> Result<()> {
    let input = r#"echo -n "3.  "$'\x{65}' | hexdump -C
echo -n "4.  "$'\x{65' | hexdump -C"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// printf formatting tests

#[test]
fn parse_printf_float() -> Result<()> {
    let input = r#"printf "%f\n" 3.14159
printf "%.2f\n" 3.14159
printf "%6.2f\n" 3.14159"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_printf_scientific() -> Result<()> {
    let input = r#"printf "%e\n" 1234.5
printf "%E\n" 1234.5"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_printf_general() -> Result<()> {
    let input = r#"printf "%g\n" 1234.5
printf "%G\n" 0.00012345"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_printf_edge_cases() -> Result<()> {
    let input = r#"printf "%e\n" 0.0
printf "%E\n" 0.0
printf "%g\n" 0.0000001
printf "%G\n" 1000000.0"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Loop construct tests

#[test]
fn parse_c_style_for_loop() -> Result<()> {
    let input = r#"for ((i=0; i<5; i++)); do echo $i; done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_for_loop_without_in() -> Result<()> {
    let input = r#"for x in a b c; do echo $x; done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_for_loop_with_extra_whitespace() -> Result<()> {
    let input = r#"for x  in  a  b  c  ; do echo $x; done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// IFS handling tests

#[test]
fn parse_ifs_newline() -> Result<()> {
    let input = r#"IFS=$'\n'
echo "test1 test2 test3" | read a b c
echo "a=$a b=$b c=$c""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ifs_tab() -> Result<()> {
    let input = r#"IFS=$'\t'
echo -e "test1\ttest2\ttest3" | read a b c
echo "a=$a b=$b c=$c""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_ifs_multiple_spaces() -> Result<()> {
    let input = r#"IFS='   '
data="a    b     c"
for word in $data; do echo "Word: $word"; done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Pattern matching tests

#[test]
fn parse_pattern_matching_character_sets() -> Result<()> {
    let input = r#"case "abc" in
  [a-z]*) echo "matches";;
  *) echo "no match";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_pattern_matching_negative_extglob() -> Result<()> {
    let input = r#"shopt -s extglob
case "hello" in
  !(*.txt)) echo "not a txt file";;
  *) echo "txt file";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Extglob tests

#[test]
fn parse_extglob_optional_patterns() -> Result<()> {
    let input = r#"shopt -s extglob
echo *(a)"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_extglob_plus_patterns() -> Result<()> {
    let input = r#"shopt -s extglob
echo +(a)"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_extglob_disabled() -> Result<()> {
    let input = r#"shopt -u extglob
echo @(*.txt|*.md)"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_extglob_escaping() -> Result<()> {
    let input = r#"shopt -s extglob
echo \@(pattern)"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Function handling tests

#[test]
fn parse_function_with_hyphen() -> Result<()> {
    let input = r#"function test-func() {
  echo "test-func called"
}
test-func"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_with_number() -> Result<()> {
    let input = r#"function "123func"() {
  echo "123func called"
}
"123func""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_function_shadowing_builtin() -> Result<()> {
    let input = r#"function echo() {
  builtin echo "shadowed: $@"
}
echo "test""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Parameter expansion tests

#[test]
fn parse_parameter_expansion_default_value() -> Result<()> {
    let input = r#"var="value"
echo "${var:-default}"
echo "${var:+alternative}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_parameter_expansion_empty_variable() -> Result<()> {
    let input = r#"value=""
echo "Default: ${value:-default}"
echo "Alternative: ${value:+alt}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Conditional expression tests

#[test]
fn parse_conditional_arithmetic_comparison() -> Result<()> {
    let input = r#"if [ $((1 + 1)) -eq 2 ]; then
  echo "true"
fi"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_conditional_string_matching() -> Result<()> {
    let input = r#"[[ "hello" == "hello" ]] && echo "match"
[[ "hello" =~ ^hell ]] && echo "regex match""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Empty and space check tests

#[test]
fn parse_empty_string_check() -> Result<()> {
    let input = r#"[[ -z "" ]] && echo "empty"
[[ -n "text" ]] && echo "not empty""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_space_matching() -> Result<()> {
    let input = r#"[[ "a b" =~ .* ]] && echo "spaces match""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// gettext style quotes

#[test]
fn parse_gettext_style_quotes() -> Result<()> {
    let input = r#"quoted=$"Hello, world"
echo "Content: [${quoted}]""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Comment handling in command substitution

#[test]
fn parse_comment_with_single_quote() -> Result<()> {
    let input = r#"echo "test # 'comment' in $(echo test)""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_comment_with_double_quote() -> Result<()> {
    let input = r#"echo "test # \"comment\" in $(echo test)""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_comment_with_parentheses() -> Result<()> {
    let input = r#"echo "test # (comment) in $(echo test)""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Case statements with extglob

#[test]
fn parse_case_with_extglob_pattern() -> Result<()> {
    let input = r#"shopt -s extglob
case "test.txt" in
  *.@(txt|md)) echo "match";;
  *) echo "no match";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_case_with_extglob_no_match() -> Result<()> {
    let input = r#"shopt -s extglob
case "test.pdf" in
  *.@(txt|md)) echo "match";;
  *) echo "no match";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Date command tests

#[test]
fn parse_simple_date_command() -> Result<()> {
    let input = r#"date "+%Y-%m-%d""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

#[test]
fn parse_date_with_complex_format() -> Result<()> {
    let input = r#"date "+%a %b %d{%Y}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// kill command test

#[test]
fn parse_kill_list_command() -> Result<()> {
    let input = r#"kill -l"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// read command test

#[test]
fn parse_read_with_empty_lines() -> Result<()> {
    let input = r#"read -a arr <<< ""
echo "arr length: ${#arr[@]}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// shopt command test

#[test]
fn parse_shopt_interactive_defaults() -> Result<()> {
    let input = r#"shopt -p | grep -E "(interactive|xtrace|verbose)""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Standalone negation test

#[test]
fn parse_standalone_negation() -> Result<()> {
    let input = r#"!"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// History command test

#[test]
fn parse_history_commands() -> Result<()> {
    let input = r#"history -c
history | head -5"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Unset command test

#[test]
fn parse_unset_odd_function_names() -> Result<()> {
    let input = r#"unset -f "test-func" "123func""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// File operation tests

#[test]
fn parse_file_operations() -> Result<()> {
    let input = r#"ls /tmp 2>/dev/null | head -1
test -f /tmp && echo "tmp exists""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// IFS with newline handling

#[test]
fn parse_ifs_newline_handling() -> Result<()> {
    let input = r#"IFS=$'\n'
data="line1
line2
line3"
for line in $data; do
  echo "Line: $line"
done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// IFS with tab handling

#[test]
fn parse_ifs_tab_handling() -> Result<()> {
    let input = r#"IFS=$'\t'
data="col1\tcol2\tcol3"
for col in $data; do
  echo "Col: $col"
done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// IFS with multiple spaces

#[test]
fn parse_ifs_multiple_spaces_with_block() -> Result<()> {
    let input = r#"IFS='   '
data="x   y    z"
for word in $data; do
  echo "Word: $word"
done"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// IFS with command substitution multiline

#[test]
fn parse_ifs_command_substitution_multiline() -> Result<()> {
    let input = r#"IFS=$'\n'
read -a arr <<< $'item1\nitem2\nitem3'
echo "Items: ${arr[@]}""#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Pattern matching with character sets

#[test]
fn parse_pattern_matching_alnum() -> Result<()> {
    let input = r#"case "test123" in
  [[:alnum:]]*) echo "alnum match";;
  *) echo "no match";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}

// Pattern matching with negative extglobs

#[test]
fn parse_pattern_matching_not_txt() -> Result<()> {
    let input = r#"shopt -s extglob
case "file.log" in
  !(*.txt)) echo "not txt";;
  *) echo "txt";;
esac"#;
    let result = test_with_snapshot(input)?;
    assert_snapshot_redacted!(ParseResult {
        input,
        result: &result
    });
    Ok(())
}
