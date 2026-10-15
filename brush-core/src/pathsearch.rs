//! Path searching utilities.

use std::path::{Path, PathBuf};

use crate::sys;
use crate::sys::fs::PathExt;

/// A search path for commands, such as the shell's `PATH`: directories to look in,
/// in order.
///
/// A relative directory is relative to the search's working directory, which needn't
/// be the process's, and an empty one is that directory, `.`, as in bash. What a
/// search finds is in the form its directory gives it: relative through a relative
/// directory, as the shell reports and runs it.
#[derive(Clone, Debug)]
pub struct SearchPath {
    dirs: Vec<PathBuf>,
    working_dir: PathBuf,
}

/// A command a search found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// The command's path, in the form the directory it was found in gives it.
    pub(crate) path: PathBuf,
    /// Whether the search passed the working directory (see [`is_working_dir`]): at or
    /// before the directory the command was found in, or anywhere in the search if it
    /// found only a file that can't be run. What such a search finds depends on the
    /// working directory, so a search from another may find something else.
    pub(crate) after_working_dir: bool,
}

impl SearchPath {
    /// Creates a search path.
    ///
    /// # Arguments
    ///
    /// * `dirs` - The directories to look in, in order.
    /// * `working_dir` - The directory relative directories are relative to.
    pub fn new<I, P>(dirs: I, working_dir: impl Into<PathBuf>) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let dirs = dirs
            .into_iter()
            .map(|dir| {
                let dir = dir.into();
                if dir.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    dir
                }
            })
            .collect();

        Self {
            dirs,
            working_dir: working_dir.into(),
        }
    }

    /// Resolves a command name the way the shell does when it's about to run it.
    ///
    /// Returns the first executable in search order, or, if there is none, the first
    /// file that exists and isn't a directory, which the shell reports as the command
    /// and then fails to run.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command to resolve.
    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        self.search(name, false).map(|found| found.path)
    }

    /// Finds the first executable with the given name, in search order.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the executable to look for.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        self.search(name, true).map(|found| found.path)
    }

    /// Searches for a command as [`Self::resolve`] does, or for an executable only, as
    /// [`Self::find`] does; and tells whether the search passed the working directory.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command to look for.
    /// * `executable_only` - Whether only an executable will do.
    pub(crate) fn search(&self, name: &str, executable_only: bool) -> Option<Found> {
        let mut fallback = None;
        let mut after_working_dir = false;

        for dir in &self.dirs {
            after_working_dir = after_working_dir || is_working_dir(dir, &self.working_dir);
            let path = dir.join(name);

            if let Some(path) = resolve_executable_in(&path, &self.working_dir) {
                return Some(Found {
                    path,
                    after_working_dir,
                });
            }

            // Remember the first non-directory entry seen, in case no executable turns up.
            if !executable_only
                && fallback.is_none()
                && self
                    .working_dir
                    .join(&path)
                    .metadata()
                    .is_ok_and(|m| !m.is_dir())
            {
                fallback = Some(path);
            }
        }

        // As in bash, the fallback counts as found after the working directory if the
        // whole search passed it, so that an executable that turns up there is looked
        // for first.
        fallback.map(|path| Found {
            path,
            after_working_dir,
        })
    }

    /// Returns the executables with the given name, in search order.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the executables to look for.
    pub fn executables(&self, name: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
        self.dirs.iter().filter_map(move |dir| {
            resolve_executable_in(&dir.join(name.as_ref()), &self.working_dir)
        })
    }

    /// Returns the executables whose names start with the given prefix, in search
    /// order.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The prefix to look for.
    /// * `case_insensitive` - Whether to match the prefix ignoring ASCII case.
    pub fn executables_with_prefix(
        &self,
        prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf> {
        let prefix = if case_insensitive {
            prefix.to_ascii_lowercase()
        } else {
            prefix.to_owned()
        };

        self.dirs.iter().flat_map(move |dir| {
            let prefix = prefix.clone();
            self.working_dir
                .join(dir)
                .read_dir()
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(move |entry| {
                    if let Ok(mut filename) = entry.file_name().into_string() {
                        if case_insensitive {
                            filename = filename.to_ascii_lowercase();
                        }

                        if !filename.starts_with(&prefix) {
                            return None;
                        }
                    }

                    let file_type = entry.file_type().ok()?;
                    ((file_type.is_file() || file_type.is_symlink()) && entry.path().executable())
                        .then(|| dir.join(entry.file_name()))
                })
        })
    }
}

/// Returns whether `dir` is the working directory: as in bash, a directory that starts
/// with `.` and is the same directory (e.g., `.`, `./`, or `../dir` from within `dir`).
///
/// # Arguments
///
/// * `dir` - The directory, relative to `working_dir` if it's relative.
/// * `working_dir` - The working directory.
pub(crate) fn is_working_dir(dir: &Path, working_dir: &Path) -> bool {
    dir.as_os_str() == "."
        || (dir.as_os_str().as_encoded_bytes().first() == Some(&b'.')
            && sys::fs::same_file(&working_dir.join(dir), working_dir))
}

/// Resolves `path`, relative to `working_dir` if it's relative, to an executable file,
/// returning it in the form `path` gives it.
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

    // Keep `path` as it's spelled (e.g., `./bin//prog`, as a script sees it in its
    // `$0`), unless resolution changed its file name.
    match resolved.file_name() {
        Some(name) if Some(name) != path.file_name() => Some(path.with_file_name(name)),
        Some(_) => Some(path.to_owned()),
        None => Some(resolved),
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

        let search_path = SearchPath::new([&first, &second], scratch.path());

        // The plain (non-executable) file wins over the directory, rather than the
        // directory being reported as the command.
        assert_eq!(search_path.resolve("prog"), Some(second.join("prog")));

        // ...and a directory is never yielded as an executable at all.
        assert_eq!(search_path.executables("prog").next(), None);

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
            SearchPath::new(["bin"], scratch.path()).search("prog", false),
            Some(Found {
                path: Path::new("bin").join("prog"),
                after_working_dir: false,
            })
        );

        Ok(())
    }

    /// An empty entry is the working directory, as `.`, so that a command found
    /// through one has a path a launcher won't search for again; and what's found at
    /// or after it is found after the working directory.
    #[test]
    fn empty_entry_is_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let elsewhere = scratch.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere)?;
        std::fs::write(elsewhere.join("other"), "")?;
        std::fs::write(scratch.path().join("prog"), "")?;

        let search_path = SearchPath::new([PathBuf::new(), elsewhere.clone()], scratch.path());
        assert_eq!(
            search_path.search("prog", false),
            Some(Found {
                path: Path::new(".").join("prog"),
                after_working_dir: true,
            })
        );
        assert_eq!(
            search_path.search("other", false),
            Some(Found {
                path: elsewhere.join("other"),
                after_working_dir: true,
            })
        );

        // A file that can't be run is the fallback of the whole search, which passed the
        // working directory here, after the file.
        let search_path = SearchPath::new([elsewhere.clone(), PathBuf::from(".")], scratch.path());
        assert_eq!(
            search_path.search("other", false),
            Some(Found {
                path: elsewhere.join("other"),
                after_working_dir: true,
            })
        );

        Ok(())
    }

    /// Two paths are the same file however they're spelled, and not if either is missing.
    #[test]
    fn same_file_compares_files_not_spellings() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let a = scratch.path().join("a");
        let b = scratch.path().join("b");
        std::fs::create_dir_all(&a)?;
        std::fs::create_dir_all(&b)?;

        assert!(sys::fs::same_file(&a, &a.join(".")));
        assert!(sys::fs::same_file(&a, &a.join("..").join("a")));
        assert!(!sys::fs::same_file(&a, &b));
        assert!(!sys::fs::same_file(&a, &a.join("missing")));
        assert!(!sys::fs::same_file(&a.join("missing"), &a.join("missing")));

        Ok(())
    }

    /// As in bash, a directory is the working directory if it starts with `.` and is the
    /// same directory, however it's spelled; what's found at or after one is found after
    /// the working directory.
    #[test]
    fn a_directory_starting_with_a_dot_may_be_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let working_dir = scratch.path().join("wd");
        let fallback = scratch.path().join("fallback");
        std::fs::create_dir_all(working_dir.join("sub"))?;
        std::fs::create_dir_all(working_dir.join(".hidden"))?;
        std::fs::create_dir_all(&fallback)?;
        std::fs::write(fallback.join("prog"), "")?;

        let after_working_dir = |dir: &str| {
            SearchPath::new([PathBuf::from(dir), fallback.clone()], &working_dir)
                .search("prog", false)
                .map(|found| found.after_working_dir)
        };

        for dir in [".", "", "./", "./.", ".//", "../wd"] {
            assert_eq!(after_working_dir(dir), Some(true), "{dir}");
        }
        for dir in ["./sub", ".hidden", ".nosuch", "..", "sub"] {
            assert_eq!(after_working_dir(dir), Some(false), "{dir}");
        }

        Ok(())
    }

    /// A file that can't be run is the fallback, found after the working directory if the
    /// whole search passed it, as in bash: an executable that turns up there is then
    /// looked for first (see `PathCache::lookup`).
    #[test]
    fn fallback_is_found_after_the_working_dir_if_the_search_passed_it() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let abs = scratch.path().join("abs");
        std::fs::create_dir_all(&abs)?;
        std::fs::write(abs.join("prog"), "")?;

        let search_path = SearchPath::new([abs.clone(), PathBuf::from(".")], scratch.path());
        assert_eq!(
            search_path.search("prog", false),
            Some(Found {
                path: abs.join("prog"),
                after_working_dir: true,
            })
        );
        assert_eq!(search_path.search("prog", true), None);

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

        let search_path = SearchPath::new([scratch.path()], scratch.path());
        for found in [
            search_path.executables("prog").next(),
            search_path.resolve("prog"),
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
