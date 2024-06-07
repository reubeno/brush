use std::{collections::HashMap, sync::Arc};

/// An environment for defined, named functions.
#[derive(Clone, Default)]
pub struct FunctionEnv {
    functions: HashMap<String, FunctionRegistration>,
}

impl FunctionEnv {
    /// Tries to retrieve the registration for a function by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to retrieve.
    pub fn get(&self, name: &str) -> Option<&FunctionRegistration> {
        self.functions.get(name)
    }

    /// Unregisters a function from the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to remove.
    pub fn remove(&mut self, name: &str) -> Option<FunctionRegistration> {
        self.functions.remove(name)
    }

    /// Updates a function registration in this environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to update.
    /// * `definition` - The new definition for the function.
    pub fn update(&mut self, name: String, definition: Arc<parser::ast::FunctionDefinition>) {
        self.functions
            .insert(name, FunctionRegistration { definition });
    }

    /// Returns an iterator over the functions registered in this environment.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &FunctionRegistration)> {
        self.functions.iter()
    }
}

/// Encapsulates a registration for a defined function.
#[derive(Clone)]
pub struct FunctionRegistration {
    /// The definition of the function.
    pub definition: Arc<parser::ast::FunctionDefinition>,
}
