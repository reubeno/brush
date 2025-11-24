//! Example demonstrating AST serialization with the `serde` feature.
//!
//! Run with: `cargo run --package brush-parser --example serde --features serde`

use brush_parser::{Parser, ParserOptions, SourceInfo};
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse a simple shell command
    let input = "echo 'Hello, World!' && ls -la";
    let reader = BufReader::new(input.as_bytes());
    let options = ParserOptions::default();
    let source_info = SourceInfo::default();

    let mut parser = Parser::new(reader, &options, &source_info);
    let program = parser.parse_program()?;

    // Serialize the AST to JSON
    let json = serde_json::to_string_pretty(&program)?;
    println!("Parsed AST:");
    println!("{json}");

    Ok(())
}
