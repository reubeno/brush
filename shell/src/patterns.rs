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

pub(crate) fn remove_largest_matching_prefix<'a>(s: &'a str, pattern: &str) -> Result<&'a str> {
    for i in (0..s.len()).rev() {
        let prefix = &s[0..=i];
        if pattern_matches(pattern, prefix)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_prefix<'a>(s: &'a str, pattern: &str) -> Result<&'a str> {
    for i in 0..s.len() {
        let prefix = &s[0..=i];
        if pattern_matches(pattern, prefix)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_largest_matching_suffix<'a>(s: &'a str, pattern: &str) -> Result<&'a str> {
    for i in 0..s.len() {
        let suffix = &s[i..];
        if pattern_matches(pattern, suffix)? {
            return Ok(&s[..i]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_suffix<'a>(s: &'a str, pattern: &str) -> Result<&'a str> {
    for i in (0..s.len()).rev() {
        let suffix = &s[i..];
        if pattern_matches(pattern, suffix)? {
            return Ok(&s[..i]);
        }
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_largest_matching_prefix() -> Result<()> {
        assert_eq!(remove_largest_matching_prefix("ooof", "")?, "ooof");
        assert_eq!(remove_largest_matching_prefix("ooof", "x")?, "ooof");
        assert_eq!(remove_largest_matching_prefix("ooof", "o")?, "oof");
        assert_eq!(remove_largest_matching_prefix("ooof", "o*o")?, "f");
        assert_eq!(remove_largest_matching_prefix("ooof", "o*")?, "");
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_prefix() -> Result<()> {
        assert_eq!(remove_smallest_matching_prefix("ooof", "")?, "ooof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "x")?, "ooof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o")?, "oof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o*o")?, "of");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o*")?, "oof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "ooof")?, "");
        Ok(())
    }

    #[test]
    fn test_remove_largest_matching_suffix() -> Result<()> {
        assert_eq!(remove_largest_matching_suffix("foo", "")?, "foo");
        assert_eq!(remove_largest_matching_suffix("foo", "x")?, "foo");
        assert_eq!(remove_largest_matching_suffix("foo", "o")?, "fo");
        assert_eq!(remove_largest_matching_suffix("foo", "o*")?, "f");
        assert_eq!(remove_largest_matching_suffix("foo", "foo")?, "");
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_suffix() -> Result<()> {
        assert_eq!(remove_smallest_matching_suffix("fooo", "")?, "fooo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "x")?, "fooo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o")?, "foo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o*o")?, "fo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o*")?, "foo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "fooo")?, "");
        Ok(())
    }
}
