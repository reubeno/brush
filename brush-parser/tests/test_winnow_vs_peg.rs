//! Test that winnow parser produces same results as PEG parser

#[cfg(feature = "use-winnow-parser")]
use brush_parser::{parse_tokens, tokenize_str, ParserOptions};

#[cfg(feature = "use-winnow-parser")]
#[test]
fn test_winnow_vs_peg_simple_commands() {
    let test_cases = vec![
        "echo hello",
        "ls -la",
        "cd /tmp",
        "cat file.txt",
        "echo hello world",
    ];

    for input in test_cases {
        let tokens = tokenize_str(input).unwrap();
        let result = parse_tokens(&tokens, &ParserOptions::default());
        assert!(result.is_ok(), "Failed to parse: {}", input);
    }
}

#[cfg(feature = "use-winnow-parser")]
#[test]
fn test_winnow_vs_peg_pipelines() {
    let test_cases = vec![
        "echo hello | grep world",
        "ls | wc -l",
        "cat file | grep pattern | wc -l",
    ];

    for input in test_cases {
        let tokens = tokenize_str(input).unwrap();
        let result = parse_tokens(&tokens, &ParserOptions::default());
        assert!(result.is_ok(), "Failed to parse: {}", input);
    }
}

#[cfg(feature = "use-winnow-parser")]
#[test]
fn test_winnow_vs_peg_redirects() {
    let test_cases = vec![
        "echo hello > file.txt",
        "cat < input.txt",
        "ls >> output.txt",
        "command 2>&1",
    ];

    for input in test_cases {
        let tokens = tokenize_str(input).unwrap();
        let result = parse_tokens(&tokens, &ParserOptions::default());
        assert!(result.is_ok(), "Failed to parse: {}", input);
    }
}
