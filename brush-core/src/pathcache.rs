//! Path cache

use crate::{error, variables};
use std::path::PathBuf;

/// A cache of paths associated with names.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathCache {
    /// The cache itself.
    cache: std::collections::HashMap<String, PathBuf>,
    /// The names whose paths a PATH search found after passing the working directory.
    #[cfg_attr(feature = "serde", serde(default))]
    found_through_working_dir: std::collections::HashSet<String>,
}

impl PathCache {
    /// Clears all elements from the cache.
    pub fn reset(&mut self) {
        self.cache.clear();
        self.found_through_working_dir.clear();
    }

    /// Returns the path associated with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to lookup.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<PathBuf> {
        self.cache.get(name.as_ref()).cloned()
    }

    /// Sets the path associated with the given name (e.g., as `hash -p` does).
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    /// * `path` - The path to associate with the name.
    pub fn set<T: Into<String>>(&mut self, name: T, path: PathBuf) {
        self.set_found(name, path, false);
    }

    /// Sets the path that a PATH search found for the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    /// * `path` - The path to associate with the name.
    /// * `through_working_dir` - Whether the search passed the working directory (an
    ///   empty or `.` entry) on the way, so that what it finds depends on it; see
    ///   [`Self::found_through_working_dir`].
    pub fn set_found<T: Into<String>>(
        &mut self,
        name: T,
        path: PathBuf,
        through_working_dir: bool,
    ) {
        let name = name.into();
        if through_working_dir {
            self.found_through_working_dir.insert(name.clone());
        } else {
            self.found_through_working_dir.remove(&name);
        }
        self.cache.insert(name, path);
    }

    /// Returns whether the given name's path was found by a PATH search that passed
    /// the working directory on the way. Such a path depends on the working directory,
    /// which may have changed since it was found.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to look up.
    pub fn found_through_working_dir<S: AsRef<str>>(&self, name: S) -> bool {
        self.found_through_working_dir.contains(name.as_ref())
    }

    /// Projects the cache into a shell value.
    pub fn to_value(&self) -> Result<variables::ShellValue, error::Error> {
        let pairs = self
            .cache
            .iter()
            .map(|(k, v)| (Some(k.to_owned()), v.to_string_lossy().to_string()))
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
        self.found_through_working_dir.remove(name.as_ref());
        self.cache.remove(name.as_ref()).is_some()
    }
}
