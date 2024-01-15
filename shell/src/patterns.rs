use anyhow::Result;

pub(crate) fn pattern_matches(pattern: &str, value: &str) -> Result<bool> {
    // TODO: pattern matching with **
    if pattern.contains("**") {
        log::error!(
            "UNIMPLEMENTED: matching with pattern '{}' against value '{}'",
            pattern,
            value
        );
        todo!("UNIMPLEMENTED: pattern matching with '**' pattern");
    }

    // TODO: Double-check use of current working dir
    let matches = glob::Pattern::new(pattern)?.matches(value);

    Ok(matches)
}

pub(crate) fn regex_matches(regex_pattern: &str, value: &str) -> Result<bool> {
    let re = regex::Regex::new(regex_pattern)?;

    // TODO: Evaluate how compatible the `regex` crate is with POSIX EREs.
    let matches = re.is_match(value);

    Ok(matches)
}
