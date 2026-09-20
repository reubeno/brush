//! Keeps the embedding snippet in the repository `README.md` identical to the one
//! compiled and run as `examples/readme.rs`.

#![cfg(test)]

const README: &str = include_str!("../../README.md");
const EXAMPLE: &str = include_str!("../examples/readme.rs");

/// Both files may be checked out with CRLF line endings on Windows.
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The first fenced `rust` block in the README.
fn readme_snippet() -> Option<String> {
    let readme = normalized(README);
    let (_, rest) = readme.split_once("```rust\n")?;
    let (snippet, _) = rest.split_once("```")?;
    Some(snippet.to_owned())
}

/// The region between the `readme:start` and `readme:end` markers, with the
/// function body's indentation removed.
fn example_snippet() -> Option<String> {
    let example = normalized(EXAMPLE);
    let (_, rest) = example.split_once("// readme:start\n")?;
    let (body, _) = rest.split_once("    // readme:end")?;
    let body = body
        .lines()
        .map(|line| line.strip_prefix("    ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    Some(body + "\n")
}

#[test]
fn readme_snippet_matches_example() {
    let readme = readme_snippet().unwrap_or_default();
    let example = example_snippet().unwrap_or_default();

    assert!(!readme.is_empty(), "no ```rust block found in README.md");
    assert!(
        !example.is_empty(),
        "no readme:start/readme:end markers found in examples/readme.rs"
    );
    assert_eq!(
        readme, example,
        "the README.md snippet differs from examples/readme.rs"
    );
}
