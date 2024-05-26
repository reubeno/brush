use std::borrow::Cow;
use std::collections::HashMap;

use crate::error;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

#[derive(Clone, Copy)]
pub enum EnvironmentLookup {
    Anywhere,
    OnlyInGlobal,
    OnlyInCurrentLocal,
    OnlyInLocal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvironmentScope {
    /// Scope local to a function instance
    Local,
    /// Globals
    Global,
    /// Transient overrides for a command invocation
    Command,
}

#[derive(Clone, Debug)]
pub struct ShellEnvironment {
    pub(crate) scopes: Vec<(EnvironmentScope, ShellVariableMap)>,
}

impl Default for ShellEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellEnvironment {
    pub fn new() -> Self {
        Self {
            scopes: vec![(EnvironmentScope::Global, ShellVariableMap::new())],
        }
    }

    pub fn push_scope(&mut self, scope_type: EnvironmentScope) {
        self.scopes.push((scope_type, ShellVariableMap::new()));
    }

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

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.iter_using_policy(EnvironmentLookup::Anywhere)
    }

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

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<(EnvironmentScope, &ShellVariable)> {
        // Look through scopes, from the top of the stack on down.
        for (scope_type, map) in self.scopes.iter().rev() {
            if let Some(var) = map.get(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }

        None
    }

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

    pub fn get_str<S: AsRef<str>>(&self, name: S) -> Option<Cow<'_, str>> {
        self.get(name.as_ref())
            .map(|(_, v)| v.value().to_cow_string())
    }

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

    pub fn unset(&mut self, name: &str) -> Result<bool, error::Error> {
        let mut local_count = 0;
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            if Self::try_unset_in_map(map, name)? {
                // If we end up finding a local in the top-most local frame, then we replace
                // it with a placeholder.
                if matches!(scope_type, EnvironmentScope::Local) && local_count == 1 {
                    map.set(
                        name,
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped)),
                    );
                }

                return Ok(true);
            }
        }

        Ok(false)
    }

    pub fn unset_index(&mut self, name: &str, index: &str) -> Result<bool, error::Error> {
        if let Some((_, var)) = self.get_mut(name) {
            var.unset_index(index)
        } else {
            Ok(false)
        }
    }

    fn try_unset_in_map(map: &mut ShellVariableMap, name: &str) -> Result<bool, error::Error> {
        match map.get(name).map(|v| v.is_readonly()) {
            Some(true) => Err(error::Error::ReadonlyVariable),
            Some(false) => Ok(map.unset(name)),
            None => Ok(false),
        }
    }

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

    pub fn set_global<N: Into<String>>(
        &mut self,
        name: N,
        var: ShellVariable,
    ) -> Result<(), error::Error> {
        self.add(name, var, EnvironmentScope::Global)
    }
}

#[derive(Clone, Debug)]
pub struct ShellVariableMap {
    variables: HashMap<String, ShellVariable>,
}

impl ShellVariableMap {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    //
    // Iterators/Getters
    //

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.variables.iter()
    }

    pub fn get(&self, name: &str) -> Option<&ShellVariable> {
        self.variables.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ShellVariable> {
        self.variables.get_mut(name)
    }

    //
    // Setters
    //

    pub fn unset(&mut self, name: &str) -> bool {
        self.variables.remove(name).is_some()
    }

    pub fn set<N: Into<String>>(&mut self, name: N, var: ShellVariable) {
        self.variables.insert(name.into(), var);
    }
}
