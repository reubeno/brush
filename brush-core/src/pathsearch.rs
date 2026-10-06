//! Path searching utilities.
//!
//! A match is reported under the name its PATH entry gave it, as bash reports it, so a match
//! found through a relative entry is relative to the shell's working directory. It can be run
//! as it is (see [`compose_std_command`](crate::commands::compose_std_command)); to look at the
//! file, resolve it with [`Shell::absolute_path`](crate::Shell::absolute_path).

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use crate::ResolvedPath;
use crate::sys;
use crate::sys::fs::PathExt;

/// A directory to search: as named in PATH, and resolved against the shell's working
/// directory. A match is looked for under the resolved name, but reported under the given
/// one, as bash reports it.
struct SearchDir {
    named: PathBuf,
    resolved: ResolvedPath,
}

impl SearchDir {
    fn new(working_dir: &ResolvedPath, dir: &Path) -> Self {
        // An empty entry names the working directory.
        let named = if dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            dir.to_owned()
        };
        let resolved = working_dir.join(&named);

        Self { named, resolved }
    }

    /// Looks for an executable named `filename` in this directory.
    fn find_executable(&self, filename: &Path) -> Option<PathBuf> {
        // Ask the platform to resolve the path to an actual executable file, which on
        // Windows may involve appending a PATHEXT extension. The helper takes ownership
        // so Unix — where no resolution is needed — can return the path unchanged
        // without allocating.
        //
        // A directory carries the execute bit on Unix but is never a command. Filter
        // the *resolved* path rather than the input: on Windows, resolution appends a
        // PATHEXT extension, so a `prog` directory must not stop `prog.exe` in the
        // same PATH entry from being found.
        let resolved = sys::fs::resolve_executable(self.resolved.join(filename))?;
        if resolved.is_dir() {
            return None;
        }

        // Carry over any extension the platform appended.
        let mut found = self.named.join(filename);
        if let Some(resolved_name) = resolved.as_path().file_name() {
            found.set_file_name(resolved_name);
        }

        Some(found)
    }
}

fn search_dirs<P, PI>(working_dir: &ResolvedPath, paths: P) -> VecDeque<SearchDir>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
{
    paths
        .into_iter()
        .map(|dir| SearchDir::new(working_dir, dir.as_ref()))
        .collect()
}

/// Encapsulates the result of a path search.
pub struct ExecutablePathSearch<N> {
    dirs: VecDeque<SearchDir>,
    filename: N,
}

impl<N> Iterator for ExecutablePathSearch<N>
where
    N: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(dir) = self.dirs.pop_front() {
            if let Some(found) = dir.find_executable(self.filename.as_ref()) {
                return Some(found);
            }
        }

        None
    }
}

pub(crate) struct ExecutablePathPrefixSearch {
    dirs: VecDeque<SearchDir>,
    queued_items: VecDeque<PathBuf>,
    filename_prefix: String,
    case_insensitive: bool,
}

impl Iterator for ExecutablePathPrefixSearch {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        // If we already found some items and queued them, then yield one now.
        if let Some(item) = self.queued_items.pop_front() {
            return Some(item);
        }

        while let Some(dir) = self.dirs.pop_front() {
            if let Ok(readdir) = dir.resolved.read_dir() {
                for entry in readdir.flatten() {
                    if let Ok(mut filename) = entry.file_name().into_string() {
                        if self.case_insensitive {
                            filename = filename.to_ascii_lowercase();
                        }

                        if !filename.starts_with(&self.filename_prefix) {
                            continue;
                        }
                    }

                    let entry_path = dir.resolved.join(entry.file_name());
                    if let Ok(file_type) = entry.file_type() {
                        if (file_type.is_file() || file_type.is_symlink())
                            && entry_path.executable()
                        {
                            self.queued_items
                                .push_back(dir.named.join(entry.file_name()));
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
/// * `working_dir` - The directory relative paths are searched from.
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the executable file to search for.
pub fn search_for_executable<P, PI, N>(
    working_dir: &ResolvedPath,
    paths: P,
    filename: N,
) -> ExecutablePathSearch<N>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    ExecutablePathSearch {
        dirs: search_dirs(working_dir, paths),
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
/// * `working_dir` - The directory relative paths are searched from.
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the command to resolve.
pub fn resolve_command<P, PI, N>(
    working_dir: &ResolvedPath,
    paths: P,
    filename: N,
) -> Option<PathBuf>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    let filename = filename.as_ref();
    let mut first_non_executable = None;

    for dir in search_dirs(working_dir, paths) {
        // Remember the first non-directory entry seen, in case no executable turns up.
        if first_non_executable.is_none()
            && dir
                .resolved
                .join(filename)
                .metadata()
                .is_ok_and(|m| !m.is_dir())
        {
            first_non_executable = Some(dir.named.join(filename));
        }

        if let Some(found) = dir.find_executable(filename) {
            return Some(found);
        }
    }

    first_non_executable
}

pub(crate) fn search_for_executable_with_prefix<P, PI>(
    working_dir: &ResolvedPath,
    paths: P,
    filename_prefix: &str,
    case_insensitive: bool,
) -> ExecutablePathPrefixSearch
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
{
    let stored_prefix = if case_insensitive {
        filename_prefix.to_ascii_lowercase()
    } else {
        filename_prefix.into()
    };

    ExecutablePathPrefixSearch {
        dirs: search_dirs(working_dir, paths),
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

        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;
        let paths = [first.as_path(), second.as_path()];

        // The plain (non-executable) file wins over the directory, rather than the
        // directory being reported as the command.
        assert_eq!(
            resolve_command(&working_dir, paths, "prog"),
            Some(second.join("prog"))
        );

        // ...and a directory is never yielded as an executable at all.
        assert_eq!(
            search_for_executable(&working_dir, paths, "prog").next(),
            None
        );

        Ok(())
    }

    /// Creates an executable called `prog` (plus the platform's executable suffix) in `dir`,
    /// returning its file name.
    fn create_executable(dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let name = PathBuf::from(format!("prog{}", std::env::consts::EXE_SUFFIX));
        let path = dir.join(&name);
        std::fs::write(&path, "")?;
        sys::fs::make_executable(&path)?;
        Ok(name)
    }

    /// A relative entry is searched from the shell's working directory, and a match is
    /// reported under the name the entry gave it.
    #[test]
    fn relative_entry_is_searched_from_the_working_directory() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let name = create_executable(&scratch.path().join("bin"))?;
        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;

        assert_eq!(
            search_for_executable(&working_dir, ["bin"], "prog").next(),
            Some(Path::new("bin").join(&name))
        );

        // From another working directory, the same entry names a different directory.
        let elsewhere = tempfile::tempdir()?;
        let elsewhere = ResolvedPath::try_from(elsewhere.path().to_owned())?;
        assert_eq!(
            search_for_executable(&elsewhere, ["bin"], "prog").next(),
            None
        );

        Ok(())
    }

    /// An empty entry names the working directory, and a match in it is reported under `.`.
    #[test]
    fn empty_entry_is_the_working_directory() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let name = create_executable(scratch.path())?;
        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;

        assert_eq!(
            search_for_executable(&working_dir, [""], "prog").next(),
            Some(Path::new(".").join(&name))
        );
        assert_eq!(
            resolve_command(&working_dir, [""], "prog"),
            Some(Path::new(".").join(&name))
        );

        Ok(())
    }

    /// An absolute entry is searched and reported as it is, whatever the working directory.
    #[test]
    fn absolute_entry_is_reported_as_given() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let bin = scratch.path().join("bin");
        let name = create_executable(&bin)?;
        let elsewhere = tempfile::tempdir()?;
        let working_dir = ResolvedPath::try_from(elsewhere.path().to_owned())?;

        assert_eq!(
            search_for_executable(&working_dir, [bin.as_path()], "prog").next(),
            Some(bin.join(&name))
        );

        Ok(())
    }

    /// Entries are searched in order: the first match is the command, and every match is
    /// yielded in turn.
    #[test]
    fn earlier_entries_take_precedence() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let name = create_executable(&scratch.path().join("first"))?;
        create_executable(&scratch.path().join("second"))?;
        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;
        let paths = ["first", "second"];

        assert_eq!(
            resolve_command(&working_dir, paths, "prog"),
            Some(Path::new("first").join(&name))
        );
        assert_eq!(
            search_for_executable(&working_dir, paths, "prog").collect::<Vec<_>>(),
            vec![
                Path::new("first").join(&name),
                Path::new("second").join(&name)
            ]
        );

        Ok(())
    }

    /// With nothing executable to find, `resolve_command` falls back to the first match
    /// that isn't a directory, still reported under its entry's name.
    #[test]
    fn non_executable_match_is_reported_under_its_entry() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir(scratch.path().join("bin"))?;
        std::fs::write(scratch.path().join("bin").join("prog"), "")?;
        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;

        assert_eq!(
            resolve_command(&working_dir, ["bin"], "prog"),
            Some(Path::new("bin").join("prog"))
        );

        Ok(())
    }

    /// The prefix search that completes command names also searches from the working
    /// directory, and reports matches under their entries' names.
    #[test]
    fn prefix_search_reports_matches_under_their_entries() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let name = create_executable(&scratch.path().join("bin"))?;
        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;

        assert_eq!(
            search_for_executable_with_prefix(&working_dir, ["bin"], "pr", false)
                .collect::<Vec<_>>(),
            vec![Path::new("bin").join(&name)]
        );

        Ok(())
    }

    /// On Windows, `PATHEXT` resolution has to run before directories are rejected: a
    /// `prog` directory must not keep `prog.bat` in the same PATH entry from being found.
    #[cfg(windows)] // ast-grep-ignore: platform-cfg-outside-sys
    #[test]
    fn same_named_directory_does_not_hide_a_pathext_match() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("prog"))?;
        std::fs::write(scratch.path().join("prog.bat"), "@echo off\r\n")?;

        let working_dir = ResolvedPath::try_from(scratch.path().to_owned())?;
        let paths = [scratch.path()];
        for found in [
            search_for_executable(&working_dir, paths, "prog").next(),
            resolve_command(&working_dir, paths, "prog"),
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
