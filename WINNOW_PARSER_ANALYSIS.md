# Winnow Parser Analysis - Failing Test Cases

## Summary

The winnow parser (experimental) has 47 failing test cases compared to the PEG parser's 10 failing test cases. This document provides the specific test cases that fail and can be used to guide parser development.

## Test Execution Results

### PEG Parser (Default)
- **Total tests**: 1613
- **Succeeded**: 1441
- **Failed**: 10
- **Known failures**: 162
- **Skipped**: 10

### Winnow Parser (Experimental)
- **Total tests**: 1613
- **Succeeded**: 1404
- **Failed**: 47
- **Known failures**: 162
- **Skipped**: 10

## Failing Test Cases with Winnow Parser

### 1. Array Operations
- **Array index assignment**: `y[${x[0]}]=10` - Complex array indexing with variable expansion
- **Array index assignment**: `y[x[1]]=11` - Array assignment with unquoted variable as index

### 2. Quoting and String Handling
- **ANSI-C quotes**: `$'\n'` - ANSI-C style escape sequences
- **ANSI-C quotes with escape sequences**: `$'\x65'`, `$'\x{65}'` - Hex escape sequences
- **gettext style quotes**: `$"Hello, world"` - Gettext-style quoting
- **Display vars with interesting chars**: Variables containing newlines and special characters

### 3. printf Formatting
- **printf %f (float)**: Floating point formatting
- **printf %e and %E (scientific)**: Scientific notation
- **printf %g and %G (general)**: General floating point formatting
- **printf scientific notation edge cases**: Edge cases with zero and very large/small numbers

### 4. Loop Constructs
- **Arithmetic for with alternate syntax**: `for ((i=0; i<5; i++))` - C-style for loops
- **for loop without in**: Basic for loops
- **for loop without in but spaces**: For loops with extra whitespace

### 5. IFS (Internal Field Separator) Handling
- **IFS only newline character**: `IFS=$'\n'`
- **IFS only tab character**: `IFS=$'\t'`
- **IFS only whitespace multiple chars**: Multiple space characters
- **IFS newline handling**: Word splitting with newlines
- **IFS tab handling**: Word splitting with tabs
- **IFS with command substitution multiline**: Complex IFS with command substitution

### 6. Pattern Matching
- **Pattern matching: character sets**: `[[:alpha:]]`, `[a-z]` etc.
- **Pattern matching: stars in negative extglobs**: `!(*.txt)` patterns
- **case with extglob pattern**: Case statements with extglob patterns
- **case with extglob no match**: Case statements that should not match

### 7. Extended Globbing (extglob)
- **Pathname expansion: Optional patterns**: `*(a)` - Zero or more
- **Pathname expansion: Plus patterns**: `+(a)` - One or more
- **Pathname expansion: extglob disabled**: Extglob when disabled
- **Extglob with escaping**: Escaped extglob patterns

### 8. Function Handling
- **Function names with interesting characters**: Functions with hyphens, numbers
- **Functions shadowing builtins**: Overriding builtin commands
- **Unset odd function names**: Unsetting functions with special names

### 9. Special Constructs
- **test: arithmetic comparison with newline in operand**: Complex test expressions
- **Standalone negation (no command)**: `!` without a command
- **Shell language syntax error (interactive)**: Syntax error handling

### 10. Command Handling
- **Simple date**: Date command with format strings
- **Date format with year**: Complex date formatting
- **kill -l**: Kill command with signal listing
- **read -a with empty lines**: Reading empty input into arrays

### 11. shopt and Configuration
- **shopt interactive defaults**: Shell option handling

### 12. History Handling
- **existing history file**: History file operations

### 13. Parameter Expansion
- **Parameter expression: advanced alternative value**: `${var:-default}`, `${var:+alternative}`

### 14. Conditional Expressions
- **Binary string matching with expansion**: `[[ "hello" == "hello" ]]`
- **Empty and space checks**: `-z`, `-n` tests

### 15. File Operations
- **File extended tests**: Various file operations

### 16. Comment Handling in Command Substitution
- **Ignore quotes in comment in command substitution**: Quote handling in comments
- **Ignore single/double quote in command substitution**: Various quote types
- **Ignore parentheses in command substitution**: Parentheses in comments

## Test Files Created

1. **winnow_parser_test_cases.txt**: Contains all 47 failing test cases in executable format
2. **test_winnow_cases.sh**: Script to test specific cases with both parsers
3. **WINNOW_PARSER_ANALYSIS.md**: This analysis document

## How to Use These Test Cases

### Test Individual Cases
```bash
# Test with PEG parser (default)
cargo run --bin brush -- -c "x=(3 2 1); y[\${x[0]}]=10; y[x[1]]=11; declare -p y"

# Test with Winnow parser (experimental)
cargo run --bin brush --features experimental-parser -- --experimental-parser -c "x=(3 2 1); y[\${x[0]}]=10; y[x[1]]=11; declare -p y"
```

### Run the Test Script
```bash
# Test specific cases with both parsers
./test_winnow_cases.sh both

# Test only with PEG parser
./test_winnow_cases.sh peg

# Test only with Winnow parser
./test_winnow_cases.sh winnow
```

### Run Full Test Suite
```bash
# Test with PEG parser
cargo test --test brush-compat-tests -- --bash-path /opt/homebrew/bin/bash

# Test with Winnow parser
cargo test --test brush-compat-tests --features experimental-parser -- --bash-path /opt/homebrew/bin/bash --brush-args="--experimental-parser"
```

## Key Areas for Winnow Parser Improvement

Based on the failing tests, these are the key areas that need attention:

1. **Array Indexing**: Complex array indexing with variable expansion
2. **ANSI-C Quoting**: Proper handling of `$'...'` escape sequences
3. **printf Formatting**: Floating point and scientific notation formatting
4. **C-style For Loops**: `for ((...))` syntax parsing
5. **IFS Handling**: Internal field separator with special characters
6. **Extglob Patterns**: Extended globbing pattern matching
7. **Function Names**: Functions with special characters
8. **Parameter Expansion**: Advanced parameter expansion syntax
9. **Pattern Matching**: Character sets and negative patterns
10. **Comment Handling**: Quote handling within comments

## Next Steps for Development

1. **Prioritize by Impact**: Focus on the most commonly used features first
2. **Create Unit Tests**: Add specific unit tests for the parser components
3. **Incremental Fixes**: Fix one category at a time and verify with tests
4. **Regression Testing**: Ensure fixes don't break existing functionality
5. **Performance Optimization**: Profile and optimize the winnow parser

The test cases provided in `winnow_parser_test_cases.txt` can be used as regression tests to verify that parser improvements don't introduce new issues.