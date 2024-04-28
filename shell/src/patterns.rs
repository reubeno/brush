use crate::error;
use std::path::{Path, PathBuf};

pub enum PatternPiece {
    Pattern(String),
    Literal(String),
}

impl PatternPiece {
    pub fn unwrap(self) -> String {
        match self {
            PatternPiece::Pattern(s) => s,
            PatternPiece::Literal(s) => s,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            PatternPiece::Pattern(s) => s,
            PatternPiece::Literal(s) => s,
        }
    }
}

pub(crate) fn pattern_expand_ex(
    pattern_pieces: &[PatternPiece],
    working_dir: &Path,
    enable_extended_globbing: bool,
) -> Result<Vec<String>, error::Error> {
    let concatenated: String = pattern_pieces.iter().map(|piece| piece.as_str()).collect();

    // FIXME: This doesn't honor quoting.
    pattern_expand(concatenated.as_str(), working_dir, enable_extended_globbing)
}

pub(crate) fn pattern_expand(
    pattern: &str,
    working_dir: &Path,
    enable_extended_globbing: bool,
) -> Result<Vec<String>, error::Error> {
    if pattern.is_empty() {
        return Ok(vec![]);
    } else if !requires_expansion(pattern) {
        return Ok(vec![pattern.to_owned()]);
    }

    let pattern_as_path = Path::new(pattern);
    let is_absolute = pattern_as_path.is_absolute();

    let prefix_to_remove;
    let mut paths_so_far = if is_absolute {
        prefix_to_remove = None;
        vec![PathBuf::new()]
    } else {
        let mut working_dir_str = working_dir.to_string_lossy().to_string();
        working_dir_str.push('/');

        prefix_to_remove = Some(working_dir_str);
        vec![working_dir.to_path_buf()]
    };

    for component in pattern_as_path {
        let component_str = component.to_string_lossy();
        if !requires_expansion(component_str.as_ref()) {
            for p in &mut paths_so_far {
                p.push(component);
            }
            continue;
        }

        let current_paths = std::mem::take(&mut paths_so_far);
        for current_path in current_paths {
            let regex =
                pattern_to_regex(component_str.as_ref(), true, true, enable_extended_globbing)?;
            let mut matching_paths_in_dir: Vec<_> = current_path
                .read_dir()
                .map_or_else(|_| vec![], |dir| dir.into_iter().collect())
                .into_iter()
                .filter_map(|result| result.ok())
                .filter(|entry| {
                    regex
                        .is_match(entry.file_name().to_string_lossy().as_ref())
                        .unwrap_or(false)
                })
                .map(|entry| entry.path())
                .collect();

            matching_paths_in_dir.sort();

            paths_so_far.append(&mut matching_paths_in_dir);
        }
    }

    let results: Vec<_> = paths_so_far
        .into_iter()
        .map(|path| {
            let path_str = path.to_string_lossy();
            let mut path_ref = path_str.as_ref();

            if let Some(prefix_to_remove) = &prefix_to_remove {
                path_ref = path_ref.strip_prefix(prefix_to_remove).unwrap();
            }

            path_ref.to_string()
        })
        .collect();

    Ok(results)
}

fn requires_expansion(s: &str) -> bool {
    // TODO: Make this more accurate.
    s.contains(|c| matches!(c, '*' | '?' | '[' | ']' | '(' | ')'))
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
) -> Result<Option<Vec<Option<String>>>, error::Error> {
    // TODO: Evaluate how compatible the `fancy_regex` crate is with POSIX EREs.
    let re = fancy_regex::Regex::new(regex_pattern)?;

    Ok(re.captures(value)?.map(|captures| {
        captures
            .iter()
            .map(|c| c.map(|m| m.as_str().to_owned()))
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
            "^a(b|c)$"
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a?(b|c)")?.as_str(),
            "^a(b|c)?$"
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a*(ab|ac)")?.as_str(),
            "^a(ab|ac)*$"
        );
        assert_eq!(
            ext_pattern_to_exact_regex_str("a+(ab|ac)")?.as_str(),
            "^a(ab|ac)+$"
        );
        assert_eq!(ext_pattern_to_exact_regex_str("[ab]")?.as_str(), "^[ab]$");
        assert_eq!(ext_pattern_to_exact_regex_str("[a-d]")?.as_str(), "^[a-d]$");
        assert_eq!(ext_pattern_to_exact_regex_str(r"\*")?.as_str(), r"^\*$");

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
