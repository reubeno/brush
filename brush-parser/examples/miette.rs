//! Simple example of miette usage

use brush_parser::{ParserOptions, SourceInfo, parse_tokens, tokenize_str};
use miette::{IntoDiagnostic, miette};

fn main() -> miette::Result<()> {
    let f = std::env::args()
        .nth(1)
        .ok_or_else(|| miette!("Please provide a file name"))?;

    let source = std::fs::read_to_string(&f).into_diagnostic()?;
    let tokens = tokenize_str(&source).into_diagnostic()?;

    let ast = parse_tokens(&tokens, &ParserOptions::default(), &SourceInfo::default())
        .map_err(|e| e.to_pretty_error(&source))?;

    println!("{ast:#?}");

    Ok(())
}
