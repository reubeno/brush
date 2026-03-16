//! Flat name → variable map, used as the per-scope storage in `ShellEnvironment`.

use std::collections::HashMap;

use crate::variables::ShellVariable;

/// Represents a map from names to shell variables.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShellVariableMap {
    variables: HashMap<String, ShellVariable>,
}

impl ShellVariableMap {
    /// Returns an iterator over all the variables in the map.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.variables.iter()
    }

    /// Tries to retrieve an immutable reference to the variable with the given name.
    pub fn get(&self, name: &str) -> Option<&ShellVariable> {
        self.variables.get(name)
    }

    /// Tries to retrieve a mutable reference to the variable with the given name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ShellVariable> {
        self.variables.get_mut(name)
    }

    /// Removes the variable with the given name, returning it if it was present.
    pub fn unset(&mut self, name: &str) -> Option<ShellVariable> {
        self.variables.remove(name)
    }

    /// Inserts a variable into the map. Variable names must not contain `[`
    /// (subscript syntax is handled by callers before reaching the map).
    pub fn set<N: Into<String>>(&mut self, name: N, var: ShellVariable) -> Option<ShellVariable> {
        let name = name.into();
        super::names::assert_bare_name(&name, "ShellVariableMap::set");
        self.variables.insert(name, var)
    }
}
