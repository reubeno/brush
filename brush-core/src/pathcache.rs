use std::path::PathBuf;

/// A cache of paths associated with names.
#[derive(Clone, Default)]
pub struct PathCache {
    /// The cache itself.
    cache: std::collections::HashMap<String, PathBuf>,
}

impl PathCache {
    /// Clears all elements from the cache.
    pub fn reset(&mut self) {
        self.cache.clear();
    }

    /// Returns the path associated with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to lookup.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<PathBuf> {
        self.cache.get(name.as_ref()).cloned()
    }

    /// Sets the path associated with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    pub fn set<S: AsRef<str>>(&mut self, name: S, path: PathBuf) {
        self.cache.insert(name.as_ref().to_string(), path);
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
