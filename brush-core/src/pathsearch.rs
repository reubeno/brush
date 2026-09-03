//! Path searching utilities.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use crate::sys;
use crate::sys::fs::PathExt;

/// Encapsulates the result of a path search.
pub struct ExecutablePathSearch<PI, N> {
    paths: VecDeque<PI>,
    filename: N,
}

impl<PI, N> Iterator for ExecutablePathSearch<PI, N>
where
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref()).join(self.filename.as_ref());

            // Ask the platform to resolve the path to an actual executable file, which on
            // Windows may involve appending a PATHEXT extension. The helper takes ownership
            // so Unix — where no resolution is needed — can return the path unchanged
            // without allocating.
            //
            // A directory carries the execute bit on Unix but is never a command. Filter
            // the *resolved* path rather than the input: on Windows, resolution appends a
            // PATHEXT extension, so a `prog` directory must not stop `prog.exe` in the
            // same PATH entry from being found.
            if let Some(resolved) = sys::fs::resolve_executable(path)
                && !resolved.is_dir()
            {
                return Some(resolved);
            }
        }

        None
    }
}

pub(crate) struct ExecutablePathPrefixSearch<PI> {
    paths: VecDeque<PI>,
    queued_items: VecDeque<PathBuf>,
    filename_prefix: String,
    case_insensitive: bool,
}

impl<PI> Iterator for ExecutablePathPrefixSearch<PI>
where
    PI: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        // If we already found some items and queued them, then yield one now.
        if let Some(item) = self.queued_items.pop_front() {
            return Some(item);
        }

        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref());

            if let Ok(readdir) = path.read_dir() {
                for entry in readdir.flatten() {
                    if let Ok(mut filename) = entry.file_name().into_string() {
                        if self.case_insensitive {
                            filename = filename.to_ascii_lowercase();
                        }

                        if !filename.starts_with(&self.filename_prefix) {
                            continue;
                        }
                    }

                    let entry_path = entry.path();
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() && entry_path.executable() {
                            self.queued_items.push_back(entry_path);
                            continue;
                        }
                        if file_type.is_symlink() && entry_path.executable() {
                            self.queued_items.push_back(entry_path);
                        }
                    }
                }
            }
            if let Some(item) = self.queued_items.pop_front() {
                return Some(item);
            }
        }

        None
    }
}

/// Search for the given executable name in the provided paths.
///
/// # Arguments
///
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the executable file to search for.
pub fn search_for_executable<P, PI, N>(paths: P, filename: N) -> ExecutablePathSearch<PI, N>
where
    P: Iterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    ExecutablePathSearch {
        paths: paths.collect(),
        filename,
    }
}

/// Resolves a command name the way the shell does when it is about to run it.
///
/// Returns the first executable in search order, or -- if there is none -- the first entry
/// that exists and is not a directory, which the shell reports as the command and then
/// fails to run.
///
/// # Arguments
///
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the command to resolve.
pub fn resolve_command<P, PI, N>(paths: P, filename: N) -> Option<PathBuf>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    let mut first_non_executable = None;

    for dir in paths {
        let path = dir.as_ref().join(filename.as_ref());

        // Remember the first non-directory entry seen, in case no executable turns up.
        if first_non_executable.is_none() && path.metadata().is_ok_and(|m| !m.is_dir()) {
            first_non_executable = Some(path.clone());
        }

        // Resolve, then reject directories; see `ExecutablePathSearch::next`.
        if let Some(resolved) = sys::fs::resolve_executable(path)
            && !resolved.is_dir()
        {
            return Some(resolved);
        }
    }

    first_non_executable
}

pub(crate) fn search_for_executable_with_prefix<P, PI>(
    paths: P,
    filename_prefix: &str,
    case_insensitive: bool,
) -> ExecutablePathPrefixSearch<PI>
where
    P: Iterator<Item = PI>,
    PI: AsRef<Path>,
{
    let stored_prefix = if case_insensitive {
        filename_prefix.to_ascii_lowercase()
    } else {
        filename_prefix.into()
    };

    ExecutablePathPrefixSearch {
        paths: paths.collect(),
        queued_items: VecDeque::new(),
        filename_prefix: stored_prefix,
        case_insensitive,
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use anyhow::Result;

    use super::*;

    /// A directory carries the execute bit on Unix, so it must not be mistaken for a
    /// command; an executable later in the search order takes its place.
    #[test]
    fn directory_is_not_a_command() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let first = scratch.path().join("first");
        let second = scratch.path().join("second");
        std::fs::create_dir_all(first.join("prog"))?;
        std::fs::create_dir_all(&second)?;
        std::fs::write(second.join("prog"), "")?;

        let paths = [first.as_path(), second.as_path()];

        // The plain (non-executable) file wins over the directory, rather than the
        // directory being reported as the command.
        assert_eq!(resolve_command(paths, "prog"), Some(second.join("prog")));

        // ...and a directory is never yielded as an executable at all.
        assert_eq!(search_for_executable(paths.iter(), "prog").next(), None);

        Ok(())
    }

    /// On Windows, `PATHEXT` resolution has to run before directories are rejected: a
    /// `prog` directory must not keep `prog.bat` in the same PATH entry from being found.
    #[cfg(windows)]
    #[test]
    fn same_named_directory_does_not_hide_a_pathext_match() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("prog"))?;
        std::fs::write(scratch.path().join("prog.bat"), "@echo off\r\n")?;

        let paths = [scratch.path()];
        for found in [
            search_for_executable(paths.iter(), "prog").next(),
            resolve_command(paths, "prog"),
        ] {
            assert!(
                found.as_ref().is_some_and(|path| path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bat"))),
                "unexpected resolution: {found:?}"
            );
        }

        Ok(())
    }
}
