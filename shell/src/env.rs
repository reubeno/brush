use anyhow::Result;
use std::collections::HashMap;

use crate::variables::{ShellValue, ShellVariable};

#[derive(Clone, Copy)]
pub enum EnvironmentLookup {
    Anywhere,
    OnlyInGlobal,
    OnlyInCurrentLocal,
    OnlyInLocal,
}

#[derive(Clone, Copy)]
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
        let mut visible_vars: HashMap<&String, &ShellVariable> = HashMap::new();

        for var_map in self.locals_stack.iter().rev() {
            for (name, var) in var_map.iter() {
                if !visible_vars.contains_key(name) {
                    visible_vars.insert(name, var);
                }
            }
        }

        for (name, var) in self.globals.iter() {
            if !visible_vars.contains_key(name) {
                visible_vars.insert(name, var);
            }
        }

        visible_vars.into_iter()
    }

    pub fn get(&self, name: &str) -> Option<&ShellVariable> {
        // First look through locals, from the top of the stack on down.
        for map in self.locals_stack.iter().rev() {
            if let Some(var) = map.get(name) {
                return Some(var);
            }
        }

        // If we didn't find it in locals, then look in globals.
        return self.globals.get(name);
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ShellVariable> {
        // First look through locals, from the top of the stack on down.
        for map in self.locals_stack.iter_mut().rev() {
            if let Some(var) = map.get_mut(name) {
                return Some(var);
            }
        }

        // If we didn't find it in locals, then look in globals.
        return self.globals.get_mut(name);
    }

    pub fn get_str(&self, name: &str) -> Option<String> {
        self.get(name).map(|v| String::from(&v.value))
    }

    pub fn is_set(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    //
    // Setters
    //

    pub fn unset(&mut self, name: &str) -> bool {
        // First look through locals, from the top of the stack on down.
        for map in self.locals_stack.iter_mut().rev() {
            if map.unset(name) {
                return true;
            }
        }

        // If we didn't find it in locals, then look in globals.
        self.globals.unset(name)
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

    //
    // TODO: Enforce 'readonly'.
    //

    pub fn update_or_add<N: AsRef<str>, V: Into<ShellValue>>(
        &mut self,
        name: N,
        value: V,
        updater: fn(&mut ShellVariable) -> Result<()>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<()> {
        if let Some(var) = self.get_mut_using_policy(name.as_ref(), lookup_policy) {
            var.value = value.into();
            updater(var)?;
        } else {
            match scope_if_creating {
                EnvironmentScope::Local => {
                    if let Some(map) = self.locals_stack.last_mut() {
                        let var = map.set(name.as_ref(), value);
                        updater(var)?;
                    } else {
                        return Err(anyhow::anyhow!(
                            "can't set local variable outside of function"
                        ));
                    }
                }
                EnvironmentScope::Global => {
                    let var = self.set_global(name.as_ref(), value);
                    updater(var)?;
                }
            };
        }

        Ok(())
    }

    pub fn set_global<N: AsRef<str>, V: Into<ShellValue>>(
        &mut self,
        name: N,
        value: V,
    ) -> &mut ShellVariable {
        self.globals.set(name, value)
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

    pub fn set<N: AsRef<str>, V: Into<ShellValue>>(
        &mut self,
        name: N,
        value: V,
    ) -> &mut ShellVariable {
        self.variables.insert(
            name.as_ref().to_owned(),
            ShellVariable {
                value: value.into(),
                exported: false,
                readonly: false,
                enumerable: true,
            },
        );

        self.variables.get_mut(name.as_ref()).unwrap()
    }
}
