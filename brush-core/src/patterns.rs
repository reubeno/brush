use crate::error;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

/// Represents a piece of a shell pattern.
#[derive(Clone, Debug)]
pub(crate) enum PatternPiece {
    /// A pattern that should be interpreted as a shell pattern.
    Pattern(String),
    /// A literal string that should be matched exactly.
    Literal(String),
}

impl PatternPiece {
    pub fn as_str(&self) -> &str {
        match self {
            PatternPiece::Pattern(s) => s,
            PatternPiece::Literal(s) => s,
        }
    }
}

type PatternWord = Vec<PatternPiece>;

/// Encapsulates a shell pattern.
#[derive(Clone, Debug)]
pub struct Pattern {
    pieces: PatternWord,
}

impl From<PatternWord> for Pattern {
    fn from(pieces: PatternWord) -> Self {
        Self { pieces }
    }
}

impl From<&PatternWord> for Pattern {
    fn from(value: &PatternWord) -> Self {
        Self {
            pieces: value.clone(),
        }
    }
}

impl From<&str> for Pattern {
    fn from(value: &str) -> Self {
        Self {
            pieces: vec![PatternPiece::Pattern(value.to_owned())],
        }
    }
}

impl From<String> for Pattern {
    fn from(value: String) -> Self {
        Self {
            pieces: vec![PatternPiece::Pattern(value)],
        }
    }
}

impl Pattern {
    /// Returns whether or not the pattern is empty.
    pub fn is_empty(&self) -> bool {
        self.pieces.iter().all(|p| p.as_str().is_empty())
    }

    /// Placeholder function that always returns true.
    pub(crate) fn accept_all_expand_filter(_path: &Path) -> bool {
        true
    }

    /// Expands the pattern into a list of matching file paths.
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The current working directory, used for relative paths.
    /// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
    /// * `path_filter` - Optionally provides a function that filters paths after expansion.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unwrap_in_result)]
    pub(crate) fn expand<PF>(
        &self,
        working_dir: &Path,
        enable_extended_globbing: bool,
        path_filter: Option<&PF>,
    ) -> Result<Vec<String>, error::Error>
    where
        PF: Fn(&Path) -> bool,
    {
        // If the pattern is completely empty, then short-circuit the function; there's
        // no reason to proceed onward when we know there's no expansions.
        if self.is_empty() {
            return Ok(vec![]);

        // Similarly, if we're *confident* the pattern doesn't require expansion, then we
        // know there's a single expansion (before filtering).
        } else if !self.pieces.iter().any(|piece| {
            matches!(piece, PatternPiece::Pattern(_)) && requires_expansion(piece.as_str())
        }) {
            let concatenated: String = self.pieces.iter().map(|piece| piece.as_str()).collect();

            if let Some(filter) = path_filter {
                if !filter(Path::new(&concatenated)) {
                    return Ok(vec![]);
                }
            }

            return Ok(vec![concatenated]);
        }

        tracing::debug!("expanding pattern: {self:?}");

        let mut components: Vec<PatternWord> = vec![];
        for piece in &self.pieces {
            let mut split_result = piece
                .as_str()
                .split(std::path::MAIN_SEPARATOR)
                .map(|s| match piece {
                    PatternPiece::Pattern(_) => PatternPiece::Pattern(s.to_owned()),
                    PatternPiece::Literal(_) => PatternPiece::Literal(s.to_owned()),
                })
                .collect::<VecDeque<_>>();

            if let Some(first_piece) = split_result.pop_front() {
                if let Some(last_component) = components.last_mut() {
                    last_component.push(first_piece);
                } else {
                    components.push(vec![first_piece]);
                }
            }

            while let Some(piece) = split_result.pop_front() {
                components.push(vec![piece]);
            }
        }

        // Check if the path appears to be absolute.
        let is_absolute = if let Some(first_component) = components.first() {
            first_component
                .iter()
                .all(|piece| piece.as_str().is_empty())
        } else {
            false
        };

        let prefix_to_remove;
        let mut paths_so_far = if is_absolute {
            prefix_to_remove = None;
            // TODO: Figure out appropriate thing to do on non-Unix platforms.
            vec![PathBuf::from(std::path::MAIN_SEPARATOR_STR)]
        } else {
            let mut working_dir_str = working_dir.to_string_lossy().to_string();

            if !working_dir_str.ends_with(std::path::MAIN_SEPARATOR) {
                working_dir_str.push(std::path::MAIN_SEPARATOR);
            }

            prefix_to_remove = Some(working_dir_str);
            vec![working_dir.to_path_buf()]
        };

        for component in components {
            if !component.iter().any(|piece| {
                matches!(piece, PatternPiece::Pattern(_)) && requires_expansion(piece.as_str())
            }) {
                for p in &mut paths_so_far {
                    let flattened = component
                        .iter()
                        .map(|piece| piece.as_str())
                        .collect::<String>();
                    p.push(flattened);
                }
                continue;
            }

            let current_paths = std::mem::take(&mut paths_so_far);
            for current_path in current_paths {
                let subpattern = Pattern::from(&component);
                let regex = subpattern.to_regex(true, true, enable_extended_globbing)?;

                let matches_regex = |dir_entry: &std::fs::DirEntry| {
                    regex
                        .is_match(dir_entry.file_name().to_string_lossy().as_ref())
                        .unwrap_or(false)
                };

                let mut matching_paths_in_dir: Vec<_> = current_path
                    .read_dir()
                    .map_or_else(|_| vec![], |dir| dir.into_iter().collect())
                    .into_iter()
                    .filter_map(|result| result.ok())
                    .filter(matches_regex)
                    .map(|entry| entry.path())
                    .collect();

                matching_paths_in_dir.sort();

                paths_so_far.append(&mut matching_paths_in_dir);
            }
        }

        let results: Vec<_> = paths_so_far
            .into_iter()
            .filter_map(|path| {
                if let Some(filter) = path_filter {
                    if !filter(path.as_path()) {
                        return None;
                    }
                }

                let path_str = path.to_string_lossy();
                let mut path_ref = path_str.as_ref();

                if let Some(prefix_to_remove) = &prefix_to_remove {
                    path_ref = path_ref.strip_prefix(prefix_to_remove).unwrap();
                }

                Some(path_ref.to_string())
            })
            .collect();

        tracing::debug!("  => results: {results:?}");

        Ok(results)
    }

    /// Converts the pattern to a regular expression string.
    ///
    /// # Arguments
    ///
    /// * `strict_prefix_match` - Whether or not the pattern should strictly match the beginning of
    ///   the string.
    /// * `strict_suffix_match` - Whether or not the pattern should strictly match the end of the
    ///   string.
    /// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
    pub(crate) fn to_regex_str(
        &self,
        strict_prefix_match: bool,
        strict_suffix_match: bool,
        enable_extended_globbing: bool,
    ) -> Result<String, error::Error> {
        let mut regex_str = String::new();

        if strict_prefix_match {
            regex_str.push('^');
        }

        for piece in &self.pieces {
            let next_part = match piece {
                PatternPiece::Pattern(s) => {
                    pattern_to_regex_str(s, false, false, enable_extended_globbing)?
                }
                PatternPiece::Literal(s) => escape_for_regex(s),
            };

            regex_str.push_str(next_part.as_str());
        }

        if strict_suffix_match {
            regex_str.push('$');
        }

        Ok(regex_str)
    }

    /// Converts the pattern to a regular expression.
    ///
    /// # Arguments
    ///
    /// * `strict_prefix_match` - Whether or not the pattern should strictly match the beginning of
    ///   the string.
    /// * `strict_suffix_match` - Whether or not the pattern should strictly match the end of the
    ///   string.
    /// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
    pub(crate) fn to_regex(
        &self,
        strict_prefix_match: bool,
        strict_suffix_match: bool,
        enable_extended_globbing: bool,
    ) -> Result<fancy_regex::Regex, error::Error> {
        let regex_str = self.to_regex_str(
            strict_prefix_match,
            strict_suffix_match,
            enable_extended_globbing,
        )?;

        tracing::debug!("pattern: '{self:?}' => regex: '{regex_str}'");

        let re = compile_regex(regex_str)?;
        Ok(re)
    }

    /// Checks if the pattern exactly matches the given string.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to check for a match.
    /// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
    pub(crate) fn exactly_matches(
        &self,
        value: &str,
        enable_extended_globbing: bool,
    ) -> Result<bool, error::Error> {
        let re = self.to_regex(true, true, enable_extended_globbing)?;
        Ok(re.is_match(value)?)
    }
}

fn requires_expansion(s: &str) -> bool {
    // TODO: Make this more accurate.
    s.contains(['*', '?', '[', ']', '(', ')'])
}

fn escape_for_regex(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        if brush_parser::pattern::regex_char_needs_escaping(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Converts a shell pattern to a regular expression.
///
/// # Arguments
///
/// * `pattern` - The shell pattern to convert.
/// * `strict_prefix_match` - Whether or not the pattern should strictly match the beginning of the
///   string.
/// * `strict_suffix_match` - Whether or not the pattern should strictly match the end of the
///   string.
/// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
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

fn pattern_to_regex_str(
    pattern: &str,
    strict_prefix_match: bool,
    strict_suffix_match: bool,
    enable_extended_globbing: bool,
) -> Result<String, error::Error> {
    // TODO: pattern matching with **
    if pattern.contains("**") {
        return error::unimp("pattern matching with '**' pattern");
    }

    let mut regex_str =
        brush_parser::pattern::pattern_to_regex_str(pattern, enable_extended_globbing)?;

    if strict_prefix_match {
        regex_str.insert(0, '^');
    }

    if strict_suffix_match {
        regex_str.push('$');
    }

    Ok(regex_str)
}

/// Removes the largest matching prefix from a string that matches the given pattern.
///
/// # Arguments
///
/// * `s` - The string to remove the prefix from.
/// * `pattern` - The pattern to match.
/// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
pub(crate) fn remove_largest_matching_prefix<'a>(
    s: &'a str,
    pattern: &Option<Pattern>,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    if let Some(pattern) = pattern {
        for i in (0..s.len()).rev() {
            let prefix = &s[0..=i];
            if pattern.exactly_matches(prefix, enable_extended_globbing)? {
                return Ok(&s[i + 1..]);
            }
        }
    }
    Ok(s)
}

/// Removes the smallest matching prefix from a string that matches the given pattern.
///
/// # Arguments
///
/// * `s` - The string to remove the prefix from.
/// * `pattern` - The pattern to match.
/// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
pub(crate) fn remove_smallest_matching_prefix<'a>(
    s: &'a str,
    pattern: &Option<Pattern>,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    if let Some(pattern) = pattern {
        for i in 0..s.len() {
            let prefix = &s[0..=i];
            if pattern.exactly_matches(prefix, enable_extended_globbing)? {
                return Ok(&s[i + 1..]);
            }
        }
    }
    Ok(s)
}

/// Removes the largest matching suffix from a string that matches the given pattern.
///
/// # Arguments
///
/// * `s` - The string to remove the suffix from.
/// * `pattern` - The pattern to match.
/// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
pub(crate) fn remove_largest_matching_suffix<'a>(
    s: &'a str,
    pattern: &Option<Pattern>,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    if let Some(pattern) = pattern {
        for i in 0..s.len() {
            let suffix = &s[i..];
            if pattern.exactly_matches(suffix, enable_extended_globbing)? {
                return Ok(&s[..i]);
            }
        }
    }
    Ok(s)
}

/// Removes the smallest matching suffix from a string that matches the given pattern.
///
/// # Arguments
///
/// * `s` - The string to remove the suffix from.
/// * `pattern` - The pattern to match.
/// * `enable_extended_globbing` - Whether or not to enable extended globbing (extglob).
pub(crate) fn remove_smallest_matching_suffix<'a>(
    s: &'a str,
    pattern: &Option<Pattern>,
    enable_extended_globbing: bool,
) -> Result<&'a str, error::Error> {
    if let Some(pattern) = pattern {
        for i in (0..s.len()).rev() {
            let suffix = &s[i..];
            if pattern.exactly_matches(suffix, enable_extended_globbing)? {
                return Ok(&s[..i]);
            }
        }
    }
    Ok(s)
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn compile_regex(regex_str: String) -> fancy_regex::Result<fancy_regex::Regex> {
    fancy_regex::Regex::new(regex_str.as_str())
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn ext_pattern_to_exact_regex_str(pattern: &str) -> Result<String, error::Error> {
        pattern_to_regex_str(pattern, true, true, true)
    }

    fn ext_pattern_str_to_exact_regex_str(word: &PatternWord) -> Result<String, error::Error> {
        Pattern::from(word).to_regex_str(true, true, true)
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
    fn test_pattern_word_translation() -> Result<()> {
        assert_eq!(
            ext_pattern_str_to_exact_regex_str(&vec![PatternPiece::Pattern("a*".to_owned())])?
                .as_str(),
            "^a.*$"
        );
        assert_eq!(
            ext_pattern_str_to_exact_regex_str(&vec![
                PatternPiece::Pattern("a*".to_owned()),
                PatternPiece::Literal("b".to_owned()),
            ])?
            .as_str(),
            "^a.*b$"
        );
        assert_eq!(
            ext_pattern_str_to_exact_regex_str(&vec![
                PatternPiece::Literal("a*".to_owned()),
                PatternPiece::Pattern("b".to_owned()),
            ])?
            .as_str(),
            r"^a\*b$"
        );

        Ok(())
    }

    #[test]
    fn test_remove_largest_matching_prefix() -> Result<()> {
        assert_eq!(
            remove_largest_matching_prefix("ooof", &Some(Pattern::from("")), true)?,
            "ooof"
        );
        assert_eq!(
            remove_largest_matching_prefix("ooof", &Some(Pattern::from("x")), true)?,
            "ooof"
        );
        assert_eq!(
            remove_largest_matching_prefix("ooof", &Some(Pattern::from("o")), true)?,
            "oof"
        );
        assert_eq!(
            remove_largest_matching_prefix("ooof", &Some(Pattern::from("o*o")), true)?,
            "f"
        );
        assert_eq!(
            remove_largest_matching_prefix("ooof", &Some(Pattern::from("o*")), true)?,
            ""
        );
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_prefix() -> Result<()> {
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("")), true)?,
            "ooof"
        );
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("x")), true)?,
            "ooof"
        );
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("o")), true)?,
            "oof"
        );
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("o*o")), true)?,
            "of"
        );
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("o*")), true)?,
            "oof"
        );
        assert_eq!(
            remove_smallest_matching_prefix("ooof", &Some(Pattern::from("ooof")), true)?,
            ""
        );
        Ok(())
    }

    #[test]
    fn test_remove_largest_matching_suffix() -> Result<()> {
        assert_eq!(
            remove_largest_matching_suffix("foo", &Some(Pattern::from("")), true)?,
            "foo"
        );
        assert_eq!(
            remove_largest_matching_suffix("foo", &Some(Pattern::from("x")), true)?,
            "foo"
        );
        assert_eq!(
            remove_largest_matching_suffix("foo", &Some(Pattern::from("o")), true)?,
            "fo"
        );
        assert_eq!(
            remove_largest_matching_suffix("foo", &Some(Pattern::from("o*")), true)?,
            "f"
        );
        assert_eq!(
            remove_largest_matching_suffix("foo", &Some(Pattern::from("foo")), true)?,
            ""
        );
        Ok(())
    }

    #[test]
    fn test_remove_smallest_matching_suffix() -> Result<()> {
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("")), true)?,
            "fooo"
        );
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("x")), true)?,
            "fooo"
        );
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("o")), true)?,
            "foo"
        );
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("o*o")), true)?,
            "fo"
        );
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("o*")), true)?,
            "foo"
        );
        assert_eq!(
            remove_smallest_matching_suffix("fooo", &Some(Pattern::from("fooo")), true)?,
            ""
        );
        Ok(())
    }
}
