use std::borrow::Cow;
use std::collections::HashMap;

use crate::error;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

/// Represents the policy for looking up variables in a shell environment.
#[derive(Clone, Copy)]
pub enum EnvironmentLookup {
    /// Look anywhere.
    Anywhere,
    /// Look only in the global scope.
    OnlyInGlobal,
    /// Look only in the current local scope.    
    OnlyInCurrentLocal,
    /// Look only in local scopes.
    OnlyInLocal,
}

/// Represents a shell environment scope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvironmentScope {
    /// Scope local to a function instance
    Local,
    /// Globals
    Global,
    /// Transient overrides for a command invocation
    Command,
}

/// Represents the shell variable environment, composed of a stack of scopes.
#[derive(Clone, Debug)]
pub struct ShellEnvironment {
    /// Stack of scopes, with the top of the stack being the current scope.
    pub(crate) scopes: Vec<(EnvironmentScope, ShellVariableMap)>,
}

impl Default for ShellEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellEnvironment {
    /// Returns a new shell environment.
    pub fn new() -> Self {
        Self {
            scopes: vec![(EnvironmentScope::Global, ShellVariableMap::new())],
        }
    }

    /// Pushes a new scope of the given type onto the environment's scope stack.
    ///
    /// # Arguments
    ///
    /// * `scope_type` - The type of scope to push.
    pub fn push_scope(&mut self, scope_type: EnvironmentScope) {
        self.scopes.push((scope_type, ShellVariableMap::new()));
    }

    /// Pops the top-most scope off the environment's scope stack.
    ///
    /// # Arguments
    ///
    /// * `expected_scope_type` - The type of scope that is expected to be atop the stack.
    pub fn pop_scope(&mut self, expected_scope_type: EnvironmentScope) -> Result<(), error::Error> {
        // TODO: Should we panic instead on failure? It's effectively a broken invariant.
        match self.scopes.pop() {
            Some((actual_scope_type, _)) if actual_scope_type == expected_scope_type => Ok(()),
            _ => Err(error::Error::MissingScope),
        }
    }

    //
    // Iterators/Getters
    //

    /// Returns an iterator over all the variables defined in the environment.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.iter_using_policy(EnvironmentLookup::Anywhere)
    }

    /// Returns an iterator over all the variables defined in the environment,
    /// using the given lookup policy.
    ///
    /// # Arguments
    ///
    /// * `lookup_policy` - The policy to use when looking up variables.
    pub fn iter_using_policy(
        &self,
        lookup_policy: EnvironmentLookup,
    ) -> impl Iterator<Item = (&String, &ShellVariable)> {
        let mut visible_vars: HashMap<&String, &ShellVariable> = HashMap::new();

        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            for (name, var) in var_map.iter() {
                if !visible_vars.contains_key(name) {
                    visible_vars.insert(name, var);
                }
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        visible_vars.into_iter()
    }

    /// Tries to retrieve an immutable reference to the variable with the given name
    /// in the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<(EnvironmentScope, &ShellVariable)> {
        // Look through scopes, from the top of the stack on down.
        for (scope_type, map) in self.scopes.iter().rev() {
            if let Some(var) = map.get(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }

        None
    }

    /// Tries to retrieve a mutable reference to the variable with the given name
    /// in the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_mut<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        // Look through scopes, from the top of the stack on down.
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if let Some(var) = map.get_mut(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }

        None
    }

    /// Tries to retrieve the string value of the variable with the given name in the
    /// environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_str<S: AsRef<str>>(&self, name: S) -> Option<Cow<'_, str>> {
        self.get(name.as_ref())
            .map(|(_, v)| v.value().to_cow_string())
    }

    /// Checks if a variable of the given name is set in the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to check.
    pub fn is_set<S: AsRef<str>>(&self, name: S) -> bool {
        if let Some((_, var)) = self.get(name) {
            !matches!(var.value(), ShellValue::Unset(_))
        } else {
            false
        }
    }

    //
    // Setters
    //

    /// Tries to unset the variable with the given name in the environment, returning
    /// whether or not such a variable existed.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to unset.
    pub fn unset(&mut self, name: &str) -> Result<Option<ShellVariable>, error::Error> {
        let mut local_count = 0;
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            let unset_result = Self::try_unset_in_map(map, name)?;

            if unset_result.is_some() {
                // If we end up finding a local in the top-most local frame, then we replace
                // it with a placeholder.
                if matches!(scope_type, EnvironmentScope::Local) && local_count == 1 {
                    map.set(
                        name,
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped)),
                    );
                }

                return Ok(unset_result);
            }
        }

        Ok(None)
    }

    /// Tries to unset an array element from the environment, using the given name and
    /// element index for lookup. Returns whether or not an element was unset.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the array variable to unset an element from.
    /// * `index` - The index of the element to unset.
    pub fn unset_index(&mut self, name: &str, index: &str) -> Result<bool, error::Error> {
        if let Some((_, var)) = self.get_mut(name) {
            var.unset_index(index)
        } else {
            Ok(false)
        }
    }

    fn try_unset_in_map(
        map: &mut ShellVariableMap,
        name: &str,
    ) -> Result<Option<ShellVariable>, error::Error> {
        match map.get(name).map(|v| v.is_readonly()) {
            Some(true) => Err(error::Error::ReadonlyVariable),
            Some(false) => Ok(map.unset(name)),
            None => Ok(None),
        }
    }

    /// Tries to retrieve an immutable reference to a variable from the environment,
    /// using the given name and lookup policy.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    pub fn get_using_policy<N: AsRef<str>>(
        &self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<&ShellVariable> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            if let Some(var) = var_map.get(name.as_ref()) {
                return Some(var);
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        None
    }

    /// Tries to retrieve a mutable reference to a variable from the environment,
    /// using the given name and lookup policy.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    pub fn get_mut_using_policy<N: AsRef<str>>(
        &mut self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<&mut ShellVariable> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            if let Some(var) = var_map.get_mut(name.as_ref()) {
                return Some(var);
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        None
    }

    /// Update a variable in the environment, or add it if it doesn't already exist.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to update or add.
    /// * `value` - The value to assign to the variable.
    /// * `updater` - A function to call to update the variable after assigning the value.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    /// * `scope_if_creating` - The scope to create the variable in if it doesn't already exist.
    pub fn update_or_add<N: Into<String>>(
        &mut self,
        name: N,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let name = name.into();

        if let Some(var) = self.get_mut_using_policy(&name, lookup_policy) {
            var.assign(value, false)?;
            updater(var)
        } else {
            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            var.assign(value, false)?;
            updater(&mut var)?;

            self.add(name, var, scope_if_creating)
        }
    }

    /// Update an array element in the environment, or add it if it doesn't already exist.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to update or add.
    /// * `index` - The index of the element to update or add.
    /// * `value` - The value to assign to the variable.
    /// * `updater` - A function to call to update the variable after assigning the value.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    /// * `scope_if_creating` - The scope to create the variable in if it doesn't already exist.
    pub fn update_or_add_array_element<N: Into<String>>(
        &mut self,
        name: N,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let name = name.into();

        if let Some(var) = self.get_mut_using_policy(&name, lookup_policy) {
            var.assign_at_index(index, value, false)?;
            updater(var)
        } else {
            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            var.assign(
                variables::ShellValueLiteral::Array(variables::ArrayLiteral(vec![(
                    Some(index),
                    value,
                )])),
                false,
            )?;
            updater(&mut var)?;

            self.add(name, var, scope_if_creating)
        }
    }

    /// Adds a variable to the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to add.
    /// * `var` - The variable to add.
    /// * `target_scope` - The scope to add the variable to.
    pub fn add<N: Into<String>>(
        &mut self,
        name: N,
        var: ShellVariable,
        target_scope: EnvironmentScope,
    ) -> Result<(), error::Error> {
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if *scope_type == target_scope {
                map.set(name, var);
                return Ok(());
            }
        }

        Err(error::Error::MissingScope)
    }

    /// Sets a global variable in the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to set.
    /// * `var` - The variable to set.
    pub fn set_global<N: Into<String>>(
        &mut self,
        name: N,
        var: ShellVariable,
    ) -> Result<(), error::Error> {
        self.add(name, var, EnvironmentScope::Global)
    }
}

/// Represents a map from names to shell variables.
#[derive(Clone, Debug)]
pub struct ShellVariableMap {
    variables: HashMap<String, ShellVariable>,
}

impl ShellVariableMap {
    /// Returns a new shell variable map.
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    //
    // Iterators/Getters
    //

    /// Returns an iterator over all the variables in the map.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.variables.iter()
    }

    /// Tries to retrieve an immutable reference to the variable with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get(&self, name: &str) -> Option<&ShellVariable> {
        self.variables.get(name)
    }

    /// Tries to retrieve a mutable reference to the variable with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ShellVariable> {
        self.variables.get_mut(name)
    }

    //
    // Setters
    //

    /// Tries to unset the variable with the given name, returning the removed
    /// variable or None if it was not already set.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to unset.
    pub fn unset(&mut self, name: &str) -> Option<ShellVariable> {
        self.variables.remove(name)
    }

    /// Sets a variable in the map.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to set.
    /// * `var` - The variable to set.
    pub fn set<N: Into<String>>(&mut self, name: N, var: ShellVariable) {
        self.variables.insert(name.into(), var);
    }
}
