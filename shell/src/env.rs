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

#[derive(Clone, Copy, Debug)]
pub enum EnvironmentScope {
    Local,
    Global,
}

#[derive(Clone)]
pub struct ShellEnvironment {
    globals: ShellVariableMap,
    locals_stack: Vec<ShellVariableMap>,
}

impl Default for ShellEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellEnvironment {
    pub fn new() -> Self {
        Self {
            globals: ShellVariableMap::new(),
            locals_stack: vec![],
        }
    }

    pub fn push_locals(&mut self) {
        self.locals_stack.push(ShellVariableMap::new());
    }

    pub fn pop_locals(&mut self) {
        self.locals_stack.pop();
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

        if !matches!(lookup_policy, EnvironmentLookup::OnlyInGlobal) {
            for var_map in self.locals_stack.iter().rev() {
                for (name, var) in var_map.iter() {
                    if !visible_vars.contains_key(name) {
                        visible_vars.insert(name, var);
                    }
                }

                if matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal) {
                    break;
                }
            }
        }

        if matches!(
            lookup_policy,
            EnvironmentLookup::Anywhere | EnvironmentLookup::OnlyInGlobal
        ) {
            for (name, var) in self.globals.iter() {
                if !visible_vars.contains_key(name) {
                    visible_vars.insert(name, var);
                }
            }
        }

        visible_vars.into_iter()
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<&ShellVariable> {
        // First look through locals, from the top of the stack on down.
        for map in self.locals_stack.iter().rev() {
            if let Some(var) = map.get(name.as_ref()) {
                return Some(var);
            }
        }

        // If we didn't find it in locals, then look in globals.
        return self.globals.get(name.as_ref());
    }

    pub fn get_mut<S: AsRef<str>>(&mut self, name: S) -> Option<&mut ShellVariable> {
        // First look through locals, from the top of the stack on down.
        for map in self.locals_stack.iter_mut().rev() {
            if let Some(var) = map.get_mut(name.as_ref()) {
                return Some(var);
            }
        }

        // If we didn't find it in locals, then look in globals.
        return self.globals.get_mut(name.as_ref());
    }

    pub fn get_str<S: AsRef<str>>(&self, name: S) -> Option<Cow<'_, str>> {
        self.get(name.as_ref()).map(|v| v.value().to_cow_string())
    }

    pub fn is_set<S: AsRef<str>>(&self, name: S) -> bool {
        if let Some(var) = self.get(name) {
            !matches!(var.value(), ShellValue::Unset(_))
        } else {
            false
        }
    }

    //
    // Setters
    //

    pub fn unset(&mut self, name: &str) -> Result<bool, error::Error> {
        // First look through locals, from the top of the stack on down.
        for (i, map) in self.locals_stack.iter_mut().rev().enumerate() {
            if Self::try_unset_in_map(map, name)? {
                // If we end up finding a local in the top-most local frame, then we replace
                // it with a placeholder.
                if i == 0 {
                    map.set(
                        name,
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped)),
                    );
                }

                return Ok(true);
            }
        }

        // If we didn't find it in locals, then look in globals.
        Self::try_unset_in_map(&mut self.globals, name)
    }

    pub fn unset_index(&mut self, name: &str, index: &str) -> Result<bool, error::Error> {
        if let Some(var) = self.get_mut(name) {
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
        match lookup_policy {
            EnvironmentLookup::Anywhere => self.get(name.as_ref()),
            EnvironmentLookup::OnlyInGlobal => self.globals.get(name.as_ref()),
            EnvironmentLookup::OnlyInCurrentLocal => {
                if let Some(map) = self.locals_stack.last() {
                    map.get(name.as_ref())
                } else {
                    None
                }
            }
            EnvironmentLookup::OnlyInLocal => {
                for map in self.locals_stack.iter().rev() {
                    if let Some(var) = map.get(name.as_ref()) {
                        return Some(var);
                    }
                }
                None
            }
        }
    }

    pub fn get_mut_using_policy<N: AsRef<str>>(
        &mut self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<&mut ShellVariable> {
        match lookup_policy {
            EnvironmentLookup::Anywhere => self.get_mut(name.as_ref()),
            EnvironmentLookup::OnlyInGlobal => self.globals.get_mut(name.as_ref()),
            EnvironmentLookup::OnlyInCurrentLocal => {
                if let Some(map) = self.locals_stack.last_mut() {
                    map.get_mut(name.as_ref())
                } else {
                    None
                }
            }
            EnvironmentLookup::OnlyInLocal => {
                for map in self.locals_stack.iter_mut().rev() {
                    if let Some(var) = map.get_mut(name.as_ref()) {
                        return Some(var);
                    }
                }
                None
            }
        }
    }

    pub fn update_or_add<N: AsRef<str>>(
        &mut self,
        name: N,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        if let Some(var) = self.get_mut_using_policy(name.as_ref(), lookup_policy) {
            var.assign(value, false)?;
            updater(var)
        } else {
            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            var.assign(value, false)?;
            updater(&mut var)?;

            self.add(name, var, scope_if_creating)
        }
    }

    pub fn update_or_add_array_element<N: AsRef<str>>(
        &mut self,
        name: N,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        if let Some(var) = self.get_mut_using_policy(name.as_ref(), lookup_policy) {
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

    pub fn add<N: AsRef<str>>(
        &mut self,
        name: N,
        var: ShellVariable,
        scope: EnvironmentScope,
    ) -> Result<(), error::Error> {
        match scope {
            EnvironmentScope::Local => {
                if let Some(map) = self.locals_stack.last_mut() {
                    map.set(name.as_ref(), var);
                } else {
                    return Err(error::Error::SetLocalVarOutsideFunction);
                }
            }
            EnvironmentScope::Global => {
                self.set_global(name.as_ref(), var);
            }
        };

        Ok(())
    }

    pub fn set_global<N: AsRef<str>>(&mut self, name: N, var: ShellVariable) {
        self.globals.set(name, var);
    }
}

#[derive(Clone)]
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

    pub fn set<N: AsRef<str>>(&mut self, name: N, var: ShellVariable) {
        self.variables.insert(name.as_ref().to_owned(), var);
    }
}
