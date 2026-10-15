//! Path cache

use crate::{error, pathsearch, variables};
use std::path::{Path, PathBuf};

/// A cache of the paths of commands, by name: the shell's hash table.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathCache {
    /// The cache itself.
    cache: std::collections::HashMap<String, CachedPath>,
}

/// A cached path.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct CachedPath {
    /// The path, as found or given.
    path: PathBuf,
    /// Whether a search found it after passing the working directory.
    after_working_dir: bool,
}

impl PathCache {
    /// Clears all elements from the cache.
    pub fn reset(&mut self) {
        self.cache.clear();
    }

    /// Looks up the path cached for the given name, as the shell does when it's about
    /// to run the command.
    ///
    /// As in bash, a cached path that depends on the working directory, which may have
    /// changed since, is looked for there first: one that a search found after passing
    /// the working directory (see [`pathsearch::Found::after_working_dir`]) is looked
    /// for as `./name`, and
    /// a relative one as `./` and the path. If that isn't there, a path in the working
    /// directory itself is treated as not hashed (though the entry is kept, as in bash);
    /// any other is returned as it is.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to look up.
    /// * `working_dir` - The shell's working directory.
    pub fn lookup<S: AsRef<str>>(&self, name: S, working_dir: &Path) -> Option<PathBuf> {
        let name = name.as_ref();
        let cached = self.cache.get(name)?;
        let relative = cached.path.is_relative();

        if cached.after_working_dir || relative {
            let tail = if relative {
                cached.path.as_path()
            } else {
                Path::new(name)
            };
            let dotted = if tail.starts_with(".") {
                tail.to_owned()
            } else {
                Path::new(".").join(tail)
            };

            if let Some(path) = pathsearch::resolve_executable_in(&dotted, working_dir) {
                return Some(path);
            }

            // As in bash, only a path starting with `.` and with a directory in it (all
            // before its last separator, as written) can be in the working directory. A
            // directory of just `.` is that, without asking the file system, for which
            // the working directory may be gone.
            if let Some(path) = cached.path.to_str()
                && path.starts_with('.')
                && let Some((dir, _)) = path.rsplit_once(std::path::is_separator)
                && pathsearch::is_working_dir(Path::new(dir), working_dir)
            {
                return None;
            }
        }

        Some(cached.path.clone())
    }

    /// Sets the path associated with the given name (e.g., as `hash -p` does).
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    /// * `path` - The path to associate with the name.
    pub fn set<T: Into<String>>(&mut self, name: T, path: PathBuf) {
        self.cache.insert(
            name.into(),
            CachedPath {
                path,
                after_working_dir: false,
            },
        );
    }

    /// Sets the path that a search found for the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    /// * `found` - What the search found.
    pub(crate) fn set_found<T: Into<String>>(&mut self, name: T, found: pathsearch::Found) {
        self.cache.insert(
            name.into(),
            CachedPath {
                path: found.path,
                after_working_dir: found.after_working_dir,
            },
        );
    }

    /// Returns the names in the table and their paths, as stored (see [`Self::lookup`]
    /// for how one is looked up as a command is run).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.cache
            .iter()
            .map(|(name, cached)| (name.as_str(), cached.path.as_path()))
    }

    /// Projects the cache into a shell value.
    pub fn to_value(&self) -> Result<variables::ShellValue, error::Error> {
        let pairs = self
            .iter()
            .map(|(name, path)| (Some(name.to_owned()), path.to_string_lossy().to_string()))
            .collect::<Vec<_>>();

        variables::ShellValue::associative_array_from_literals(variables::ArrayLiteral(pairs))
    }

    /// Removes the path associated with the given name, if there is one.
    /// Returns whether or not an entry was removed.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to remove.
    pub fn unset<S: AsRef<str>>(&mut self, name: S) -> bool {
        self.cache.remove(name.as_ref()).is_some()
    }
}

// (Unix only: the test's executables have no extension.)
#[cfg(test)]
#[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;

    /// As in bash: what a search found after passing the working directory is looked for
    /// there first, as is a relative path; a path in the working directory that's gone
    /// isn't cached; others are as cached.
    #[test]
    fn lookup_looks_in_the_working_dir_first_where_bash_does() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = tempfile::tempdir()?;
        let here = scratch.path().join("here");
        let elsewhere = scratch.path().join("elsewhere");
        std::fs::create_dir_all(here.join("bin"))?;
        std::fs::create_dir_all(&elsewhere)?;
        for path in [here.join("prog"), here.join("bin").join("prog")] {
            std::fs::write(&path, "")?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }

        let found = |path: &str, after_working_dir| pathsearch::Found {
            path: PathBuf::from(path),
            after_working_dir,
        };
        let mut cache = PathCache::default();

        // Found after passing the working directory: the working directory's, if any.
        cache.set_found("prog", found("/usr/bin/prog", true));
        assert_eq!(cache.lookup("prog", &here), Some(PathBuf::from("./prog")));
        assert_eq!(
            cache.lookup("prog", &elsewhere),
            Some(PathBuf::from("/usr/bin/prog"))
        );

        // Found in the working directory: gone with it.
        cache.set_found("prog", found("./prog", true));
        assert_eq!(cache.lookup("prog", &here), Some(PathBuf::from("./prog")));
        assert_eq!(cache.lookup("prog", &elsewhere), None);

        // Relative: as `./` and the path, if it's there; as cached, if not.
        cache.set_found("prog", found("bin/prog", false));
        assert_eq!(
            cache.lookup("prog", &here),
            Some(PathBuf::from("./bin/prog"))
        );
        assert_eq!(
            cache.lookup("prog", &elsewhere),
            Some(PathBuf::from("bin/prog"))
        );

        // Given with `hash -p`, spelled as given.
        cache.set("prog", PathBuf::from("./././prog"));
        assert_eq!(
            cache.lookup("prog", &here),
            Some(PathBuf::from("./././prog"))
        );
        cache.set("prog", PathBuf::from("./nosuch/"));
        assert_eq!(
            cache.lookup("prog", &here),
            Some(PathBuf::from("./nosuch/"))
        );

        // Given with `hash -p`: as given, even starting with `.` (with no directory).
        cache.set("prog", PathBuf::from(".hidden"));
        assert_eq!(cache.lookup("prog", &here), Some(PathBuf::from(".hidden")));
        cache.set("prog", PathBuf::from("/usr/bin/true"));
        assert_eq!(
            cache.lookup("prog", &here),
            Some(PathBuf::from("/usr/bin/true"))
        );

        Ok(())
    }
}
