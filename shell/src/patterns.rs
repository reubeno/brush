use crate::error;
use std::path::{Path, PathBuf};

pub(crate) fn pattern_expand(
    pattern: &str,
    working_dir: &Path,
) -> Result<Vec<PathBuf>, error::Error> {
    if pattern.is_empty() {
        return Ok(vec![]);
    }

    // Workaround to deal with effective working directory being different from
    // the actual process's working directory.
    let prefix_to_remove;
    let glob_pattern = if pattern.starts_with('/') {
        prefix_to_remove = None;
        pattern.to_string()
    } else {
        let mut working_dir_str = working_dir.to_string_lossy().to_string();
        if !working_dir_str.ends_with('/') {
            working_dir_str.push('/');
        }
        prefix_to_remove = Some(working_dir_str);
        working_dir.join(pattern).to_string_lossy().to_string()
    };

    let options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };

    let paths = glob::glob_with(glob_pattern.as_str(), options)
        .map_err(|_e| error::Error::InvalidPattern(pattern.to_owned()))?;
    let paths_results: Result<Vec<_>, glob::GlobError> = paths.collect();
    let mut paths = paths_results?;

    if let Some(prefix_to_remove) = prefix_to_remove {
        paths = paths
            .into_iter()
            .map(|p| {
                let rel = p
                    .to_string_lossy()
                    .strip_prefix(&prefix_to_remove)
                    .unwrap()
                    .to_owned();

                PathBuf::from(rel)
            })
            .collect();
    }

    Ok(paths)
}

pub(crate) fn pattern_exactly_matches(
    pattern: &str,
    value: &str,
    enable_extended_globbing: bool,
) -> Result<bool, error::Error> {
    let re = pattern_to_regex(pattern, true, true, enable_extended_globbing)?;
    Ok(re.is_match(value)?)
}

pub(crate) fn pattern_to_regex(
    pattern: &str,
    strict_prefix_match: bool,
    strict_suffix_match: bool,
    enable_extended_globbing: bool,
) -> Result<fancy_regex::Regex, error::Error> {
    let regex_str = pattern_to_regex_str(
        pattern,
        strict_prefix_match,
        strict_suffix_match,
        enable_extended_globbing,
    )?;

    let re = fancy_regex::Regex::new(regex_str.as_str())?;
    Ok(re)
}

pub(crate) fn pattern_to_regex_str(
    pattern: &str,
    strict_prefix_match: bool,
    strict_suffix_match: bool,
    enable_extended_globbing: bool,
) -> Result<String, error::Error> {
    // TODO: pattern matching with **
    if pattern.contains("**") {
        return error::unimp("pattern matching with '**' pattern");
    }

    let mut regex_str = parser::pattern::pattern_to_regex_str(pattern, enable_extended_globbing)?;

    if strict_prefix_match {
        regex_str.insert(0, '^');
    }

    if strict_suffix_match {
        regex_str.push('$');
    }

    Ok(regex_str)
}

pub(crate) fn regex_matches(
    regex_pattern: &str,
    value: &str,
) -> Result<Option<Vec<String>>, error::Error> {
    // TODO: Evaluate how compatible the `fancy_regex` crate is with POSIX EREs.
    let re = fancy_regex::Regex::new(regex_pattern)?;

    Ok(re.captures(value)?.map(|captures| {
        captures
            .iter()
            .map(|c| c.unwrap().as_str().to_owned())
            .collect()
    }))
}

pub(crate) fn remove_largest_matching_prefix<'a>(
    s: &'a str,
    pattern: &str,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    for i in (0..s.len()).rev() {
        let prefix = &s[0..=i];
        if pattern_exactly_matches(pattern, prefix, enable_extended_globbing)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_prefix<'a>(
    s: &'a str,
    pattern: &str,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    for i in 0..s.len() {
        let prefix = &s[0..=i];
        if pattern_exactly_matches(pattern, prefix, enable_extended_globbing)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_largest_matching_suffix<'a>(
    s: &'a str,
    pattern: &str,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    for i in 0..s.len() {
        let suffix = &s[i..];
        if pattern_exactly_matches(pattern, suffix, enable_extended_globbing)? {
            return Ok(&s[..i]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_suffix<'a>(
    s: &'a str,
    pattern: &str,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    for i in (0..s.len()).rev() {
        let suffix = &s[i..];
        if pattern_exactly_matches(pattern, suffix, enable_extended_globbing)? {
            return Ok(&s[..i]);
        }
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn ext_pattern_to_exact_regex_str(pattern: &str) -> Result<String, error::Error> {
        pattern_to_regex_str(pattern, true, true, true)
    }

    #[test]
    fn test_pattern_translation() -> Result<()> {
        assert_eq!(ext_pattern_to_exact_regex_str("a")?.as_str(), "^a$");
        assert_eq!(ext_pattern_to_exact_regex_str("a*")?.as_str(), "^a.*$");
        assert_eq!(ext_pattern_to_exact_regex_str("a?")?.as_str(), "^a.$");
        assert_eq!(
            ext_pattern_to_exact_regex_str("a@(b|c)")?.as_str(),
            "^a(b|c)$",
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a?(b|c)")?.as_str(),
            "^a(b|c)?$",
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a*(ab|ac)")?.as_str(),
            "^a(ab|ac)*$",
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a+(ab|ac)")?.as_str(),
            "^a(ab|ac)+$",
        );

        Ok(())
    }

    #[test]
    fn test_remove_largest_matching_prefix() -> Result<()> {
        assert_eq!(remove_largest_matching_prefix("ooof", "", true)?, "ooof");
        assert_eq!(remove_largest_matching_prefix("ooof", "x", true)?, "ooof");
        assert_eq!(remove_largest_matching_prefix("ooof", "o", true)?, "oof");
        assert_eq!(remove_largest_matching_prefix("ooof", "o*o", true)?, "f");
        assert_eq!(remove_largest_matching_prefix("ooof", "o*", true)?, "");
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_prefix() -> Result<()> {
        assert_eq!(remove_smallest_matching_prefix("ooof", "", true)?, "ooof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "x", true)?, "ooof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o", true)?, "oof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o*o", true)?, "of");
        assert_eq!(remove_smallest_matching_prefix("ooof", "o*", true)?, "oof");
        assert_eq!(remove_smallest_matching_prefix("ooof", "ooof", true)?, "");
        Ok(())
    }

    #[test]
    fn test_remove_largest_matching_suffix() -> Result<()> {
        assert_eq!(remove_largest_matching_suffix("foo", "", true)?, "foo");
        assert_eq!(remove_largest_matching_suffix("foo", "x", true)?, "foo");
        assert_eq!(remove_largest_matching_suffix("foo", "o", true)?, "fo");
        assert_eq!(remove_largest_matching_suffix("foo", "o*", true)?, "f");
        assert_eq!(remove_largest_matching_suffix("foo", "foo", true)?, "");
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_suffix() -> Result<()> {
        assert_eq!(remove_smallest_matching_suffix("fooo", "", true)?, "fooo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "x", true)?, "fooo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o", true)?, "foo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o*o", true)?, "fo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "o*", true)?, "foo");
        assert_eq!(remove_smallest_matching_suffix("fooo", "fooo", true)?, "");
        Ok(())
    }
}
