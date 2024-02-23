use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::error;

pub(crate) fn pattern_expand(
    pattern: &str,
    working_dir: &Path,
) -> Result<Vec<PathBuf>, error::Error> {
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
    let mut paths = paths_results.map_err(|e| error::Error::Unknown(e.into()))?;

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

pub(crate) fn pattern_matches(pattern: &str, value: &str) -> Result<bool, error::Error> {
    // TODO: pattern matching with **
    if pattern.contains("**") {
        log::error!(
            "UNIMPLEMENTED: matching with pattern '{}' against value '{}'",
            pattern,
            value
        );
        return error::unimp("pattern matching with '**' pattern");
    }

    // TODO: Double-check use of current working dir
    let matches = glob::Pattern::new(pattern)
        .map_err(|e| error::Error::Unknown(e.into()))?
        .matches(value);

    Ok(matches)
}

pub(crate) fn regex_matches(regex_pattern: &str, value: &str) -> Result<bool, error::Error> {
    let re = regex::Regex::new(regex_pattern).map_err(|e| error::Error::Unknown(e.into()))?;

    // TODO: Evaluate how compatible the `regex` crate is with POSIX EREs.
    let matches = re.is_match(value);

    Ok(matches)
}

pub(crate) fn remove_largest_matching_prefix<'a>(
    s: &'a str,
    pattern: &str,
) -> Result<&'a str, error::Error> {
    for i in (0..s.len()).rev() {
        let prefix = &s[0..=i];
        if pattern_matches(pattern, prefix)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_prefix<'a>(
    s: &'a str,
    pattern: &str,
) -> Result<&'a str, error::Error> {
    for i in 0..s.len() {
        let prefix = &s[0..=i];
        if pattern_matches(pattern, prefix)? {
            return Ok(&s[i + 1..]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_largest_matching_suffix<'a>(
    s: &'a str,
    pattern: &str,
) -> Result<&'a str, error::Error> {
    for i in 0..s.len() {
        let suffix = &s[i..];
        if pattern_matches(pattern, suffix)? {
            return Ok(&s[..i]);
        }
    }
    Ok(s)
}

pub(crate) fn remove_smallest_matching_suffix<'a>(
    s: &'a str,
    pattern: &str,
) -> Result<&'a str, error::Error> {
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
