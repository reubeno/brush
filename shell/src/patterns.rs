use anyhow::Result;

pub(crate) fn pattern_matches(pattern: &str, value: &str) -> Result<bool> {
    // TODO: pattern matching with **
    if pattern.contains("**") {
        log::error!(
            "UNIMPLEMENTED: matching with pattern '{}' against value '{}'",
            pattern,
            value
        );
        todo!("pattern matching with '**' pattern");
    }

    // TODO: Double-check use of current working dir
    let matches = glob::Pattern::new(pattern)?.matches(value);

    Ok(matches)
}
