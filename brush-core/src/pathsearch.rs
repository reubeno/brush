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
    working_dir: PathBuf,
}

impl<PI, N> Iterator for ExecutablePathSearch<PI, N>
where
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(dir) = self.paths.pop_front() {
            let path = entry_path(dir.as_ref(), self.filename.as_ref());
            if let Some(resolved) = resolve_executable_in(&path, &self.working_dir) {
                return Some(resolved);
            }
        }

        None
    }
}

/// Returns the path to `name` in the PATH entry `dir`. An empty entry is the working
/// directory, `.`, as in bash.
fn entry_path(dir: &Path, name: impl AsRef<Path>) -> PathBuf {
    if dir.as_os_str().is_empty() {
        Path::new(".").join(name)
    } else {
        dir.join(name)
    }
}

/// Returns whether a search of `paths` for `name` passes the working directory (an
/// empty or `.` entry) before, or when, reaching the entry that `found` is in. What
/// such a search finds depends on the working directory, so a shell can't keep it as
/// found. A `found` that isn't `name` in one of the entries (e.g., one hashed with
/// `hash -p`) isn't one a search found, so it doesn't.
pub(crate) fn search_passes_working_dir<P, PI>(paths: P, name: &str, found: &Path) -> bool
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
{
    // What's found may have gained an extension (as on Windows).
    let is_name = found.file_name().is_some_and(|f| f == name)
        || found.file_stem().is_some_and(|f| f == name);
    let Some(found_in) = found.parent().filter(|_| is_name) else {
        return false;
    };

    let mut passed_working_dir = false;
    for dir in paths {
        let dir = dir.as_ref();
        let is_working_dir = dir.as_os_str().is_empty() || dir == Path::new(".");
        passed_working_dir |= is_working_dir;

        let entry = if is_working_dir { Path::new(".") } else { dir };
        if entry == found_in {
            return passed_working_dir;
        }
    }

    false
}

/// Resolves `path`, relative to `working_dir` if it's relative, to an executable file,
/// returning it in the form `path` gives it. (The shell's working directory needn't be
/// the process's.)
pub(crate) fn resolve_executable_in(path: &Path, working_dir: &Path) -> Option<PathBuf> {
    // Ask the platform to resolve the path to an actual executable file, which on
    // Windows may involve appending a PATHEXT extension; that's all it changes, so
    // `path` takes on the resolved file name.
    //
    // A directory carries the execute bit on Unix but is never a command. Filter
    // the *resolved* path rather than the input: on Windows, resolution appends a
    // PATHEXT extension, so a `prog` directory must not stop `prog.exe` in the
    // same PATH entry from being found.
    let resolved = sys::fs::resolve_executable(working_dir.join(path))?;
    if resolved.is_dir() {
        return None;
    }

    Some(match resolved.file_name() {
        Some(name) => path.with_file_name(name),
        None => resolved,
    })
}

pub(crate) struct ExecutablePathPrefixSearch<PI> {
    paths: VecDeque<PI>,
    queued_items: VecDeque<PathBuf>,
    filename_prefix: String,
    case_insensitive: bool,
    working_dir: PathBuf,
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

        while let Some(dir) = self.paths.pop_front() {
            let dir = dir.as_ref();

            if let Ok(readdir) = self.working_dir.join(dir).read_dir() {
                for entry in readdir.flatten() {
                    if let Ok(mut filename) = entry.file_name().into_string() {
                        if self.case_insensitive {
                            filename = filename.to_ascii_lowercase();
                        }

                        if !filename.starts_with(&self.filename_prefix) {
                            continue;
                        }
                    }

                    if let Ok(file_type) = entry.file_type()
                        && (file_type.is_file() || file_type.is_symlink())
                        && entry.path().executable()
                    {
                        self.queued_items
                            .push_back(entry_path(dir, entry.file_name()));
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
/// * `working_dir` - The directory a relative path is relative to. An executable
///   found through one is yielded relative, as the shell reports and runs it.
pub fn search_for_executable<P, PI, N>(
    paths: P,
    filename: N,
    working_dir: &Path,
) -> ExecutablePathSearch<PI, N>
where
    P: Iterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    ExecutablePathSearch {
        paths: paths.collect(),
        filename,
        working_dir: working_dir.to_owned(),
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
/// * `working_dir` - The directory a relative path is relative to. A command found
///   through one is returned relative, as the shell reports and runs it.
pub fn resolve_command<P, PI, N>(paths: P, filename: N, working_dir: &Path) -> Option<PathBuf>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    let mut first_non_executable = None;

    for dir in paths {
        let path = entry_path(dir.as_ref(), filename.as_ref());

        // Remember the first non-directory entry seen, in case no executable turns up.
        if first_non_executable.is_none()
            && working_dir
                .join(&path)
                .metadata()
                .is_ok_and(|m| !m.is_dir())
        {
            first_non_executable = Some(path.clone());
        }

        if let Some(resolved) = resolve_executable_in(&path, working_dir) {
            return Some(resolved);
        }
    }

    first_non_executable
}

pub(crate) fn search_for_executable_with_prefix<P, PI>(
    paths: P,
    filename_prefix: &str,
    case_insensitive: bool,
    working_dir: &Path,
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
        working_dir: working_dir.to_owned(),
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
        assert_eq!(
            resolve_command(paths, "prog", scratch.path()),
            Some(second.join("prog"))
        );

        // ...and a directory is never yielded as an executable at all.
        assert_eq!(
            search_for_executable(paths.iter(), "prog", scratch.path()).next(),
            None
        );

        Ok(())
    }

    /// A relative entry is relative to the given working directory, which needn't be the
    /// process's; a command found through one is returned relative.
    #[test]
    fn relative_entry_is_relative_to_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("bin"))?;
        std::fs::write(scratch.path().join("bin").join("prog"), "")?;

        assert_eq!(
            resolve_command(["bin"], "prog", scratch.path()),
            Some(Path::new("bin").join("prog"))
        );

        Ok(())
    }

    /// A search passes the working directory if an empty or `.` entry comes at or
    /// before the one that found the command; a command no entry gives wasn't found
    /// by a search at all.
    #[test]
    fn search_passes_working_dir_at_or_before_the_entry_found() {
        let found = Path::new("/usr/bin").join("prog");
        let in_working_dir = Path::new(".").join("prog");

        assert!(search_passes_working_dir(
            ["", "/usr/bin"],
            "prog",
            &in_working_dir
        ));
        assert!(search_passes_working_dir([".", "/usr/bin"], "prog", &found));
        assert!(search_passes_working_dir(["", "/usr/bin"], "prog", &found));
        assert!(!search_passes_working_dir(["/usr/bin", ""], "prog", &found));
        assert!(!search_passes_working_dir(
            ["bin", "/usr/bin"],
            "prog",
            &found
        ));
        // As with `hash -p /usr/bin/other prog`.
        assert!(!search_passes_working_dir(
            ["", "/usr/bin"],
            "prog",
            &Path::new("/usr/bin").join("other")
        ));
        // What's found may have gained an extension (as on Windows).
        assert!(search_passes_working_dir(
            ["", "/usr/bin"],
            "prog",
            &Path::new("/usr/bin").join("prog.exe")
        ));
        assert!(!search_passes_working_dir(
            [".", "/usr/bin"],
            "prog",
            Path::new("/elsewhere/prog")
        ));
    }

    /// An empty entry is the working directory, as `.`, so that a command found
    /// through one has a path a launcher won't search for again.
    #[test]
    fn empty_entry_is_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::write(scratch.path().join("prog"), "")?;

        assert_eq!(
            resolve_command([""], "prog", scratch.path()),
            Some(Path::new(".").join("prog"))
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

        let paths = [scratch.path()];
        for found in [
            search_for_executable(paths.iter(), "prog", scratch.path()).next(),
            resolve_command(paths, "prog", scratch.path()),
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
