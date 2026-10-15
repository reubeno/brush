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
        let mut first_non_executable = None;

        for dir in &self.dirs {
            let path = dir.join(name);

            // Remember the first non-directory entry seen, in case no executable turns up.
            if first_non_executable.is_none()
                && self
                    .working_dir
                    .join(&path)
                    .metadata()
                    .is_ok_and(|m| !m.is_dir())
            {
                first_non_executable = Some(path.clone());
            }

            if let Some(path) = resolve_executable_in(&path, &self.working_dir) {
                return Some(path);
            }
        }

        first_non_executable
    }

    /// Finds the first executable with the given name, in search order.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the executable to look for.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        self.executables(name).next()
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

/// Resolves `path`, relative to `working_dir` if it's relative, to an executable file,
/// returning it in the form `path` gives it.
fn resolve_executable_in(path: &Path, working_dir: &Path) -> Option<PathBuf> {
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

    /// An empty entry is the working directory, as `.`, so that a command found through
    /// one has a path a launcher won't search for again.
    #[test]
    fn empty_entry_is_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::write(scratch.path().join("prog"), "")?;

        let search_path = SearchPath::new([PathBuf::new()], scratch.path());
        assert_eq!(
            search_path.resolve("prog"),
            Some(Path::new(".").join("prog"))
        );

        Ok(())
    }

    /// A relative entry is relative to the given working directory, which needn't be the
    /// process's; a command found through one is returned relative, as it's spelled.
    #[test]
    fn relative_entry_is_relative_to_the_working_dir() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("bin"))?;
        std::fs::write(scratch.path().join("bin").join("prog"), "")?;

        assert_eq!(
            SearchPath::new(["bin"], scratch.path()).resolve("prog"),
            Some(Path::new("bin").join("prog"))
        );
        assert_eq!(
            SearchPath::new(["./bin//"], scratch.path()).resolve("prog"),
            Some(PathBuf::from("./bin//prog"))
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
