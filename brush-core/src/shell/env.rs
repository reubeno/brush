//! Environment support for shell.

use std::borrow::Cow;

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Tries to retrieve a variable from the shell's environment, converting it into its
    /// string form.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_str(&self, name: &str) -> Option<Cow<'_, str>> {
        self.env.get_resolved_str(name, self)
    }

    /// Tries to retrieve a variable from the shell's environment, resolving namerefs.
    ///
    /// Returns a [`ResolvedVarRef`](crate::env::ResolvedVarRef) with safe accessors:
    /// - [`base_var()`](crate::env::ResolvedVarRef::base_var) for attribute/type checks
    /// - [`value_str()`](crate::env::ResolvedVarRef::value_str) for value extraction
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_resolved_var(&self, name: &str) -> Option<crate::env::ResolvedVarRef<'_>> {
        self.env.get_resolved(name)
    }

    /// Checks whether a variable of the given name is set in the shell's
    /// environment, resolving namerefs transparently.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to check.
    pub fn env_is_set(&self, name: &str) -> bool {
        self.env.is_resolved_set(name, self)
    }
}
