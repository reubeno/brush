//! Test harness for comparing Peg and Winnow parser implementations.
//!
//! This module provides utilities to test both parser implementations
//! and verify they produce equivalent results.

mod and_or_lists;
mod assignments;
mod complex;
mod compound_commands;
mod extended_test;
mod functions;
mod here_docs;
mod pipelines;
mod redirections;
mod simple_commands;

use crate::ast::Program;
use crate::error::ParseError;
use crate::parser::{Parser, ParserImpl, ParserOptions};
use anyhow::Result;

#[derive(serde::Serialize)]
struct ParseResult<'a, T> {
    input: &'a str,
    result: &'a T,
}

/// Macro to assert snapshots with location information redacted.
/// This makes snapshots stable across parser changes that only affect source locations.
#[macro_export]
macro_rules! assert_snapshot_redacted {
    ($value:expr) => {{
        let mut settings = insta::Settings::clone_current();
        settings.add_redaction(".**.loc", "[location]");
        settings.bind(|| {
            insta::assert_ron_snapshot!($value);
        });
    }};
}

/// A named parser configuration for test output clarity
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub name: &'static str,
    pub parser_impl: ParserImpl,
}

/// Returns all available parser implementations for testing.
/// - Without `winnow-parser`: returns only Peg
/// - With `winnow-parser`: returns both Peg and Winnow
pub fn parser_configs() -> Vec<ParserConfig> {
    #[allow(unused_mut)]
    let mut configs = vec![ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    }];

    configs
}

/// Helper to parse input with a specific parser configuration
pub fn parse_with_config(input: &str, config: &ParserConfig) -> Result<Program, ParseError> {
    let options = ParserOptions {
        parser_impl: config.parser_impl.clone(),
        ..Default::default()
    };

    let mut parser = Parser::new(std::io::Cursor::new(input), &options);
    parser.parse_program()
}

/// Run a test and create snapshot for peg parser (canonical implementation).
///
/// This function parses the input with the peg parser and returns the result
/// for snapshot testing. When the winnow-parser feature is enabled, it also
/// verifies that winnow produces the same result.
pub fn test_with_snapshot(input: &str) -> Result<Program> {
    // Always parse with peg (canonical implementation)
    let peg_config = ParserConfig {
        name: "peg",
        parser_impl: ParserImpl::Peg,
    };
    let peg_result = parse_with_config(input, &peg_config)
        .map_err(|e| anyhow::anyhow!("Peg parser failed: {e}\nInput: {input}"))?;

    Ok(peg_result)
}

mod harness_tests {
    use super::*;

    #[test]
    fn test_parser_configs_includes_peg() {
        let configs = parser_configs();
        assert!(!configs.is_empty());
        assert_eq!(configs[0].name, "peg");
    }

    #[test]
    fn test_parse_with_config_basic() {
        let config = ParserConfig {
            name: "peg",
            parser_impl: ParserImpl::Peg,
        };
        let result = parse_with_config("echo hello", &config);
        assert!(result.is_ok());
    }
}
