//! Structures for managing function registrations.

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

    /// Tries to retrieve a mutable reference to the registration for a
    /// function by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to retrieve.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut FunctionRegistration> {
        self.functions.get_mut(name)
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
    /// * `registration` - The new registration for the function.
    pub fn update(&mut self, name: String, registration: FunctionRegistration) {
        self.functions.insert(name, registration);
    }

    /// Clear all functions in this environment.
    pub fn clear(&mut self) {
        self.functions.clear();
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
    pub(crate) definition: Arc<brush_parser::ast::FunctionDefinition>,
    /// Whether or not this function definition should be exported to children.
    exported: bool,
}

impl From<brush_parser::ast::FunctionDefinition> for FunctionRegistration {
    fn from(definition: brush_parser::ast::FunctionDefinition) -> Self {
        Self {
            definition: Arc::new(definition),
            exported: false,
        }
    }
}

impl FunctionRegistration {
    /// Returns a reference to the function definition.
    pub fn definition(&self) -> &brush_parser::ast::FunctionDefinition {
        &self.definition
    }

    /// Marks the function for export.
    pub const fn export(&mut self) {
        self.exported = true;
    }

    /// Unmarks the function for export.
    pub const fn unexport(&mut self) {
        self.exported = false;
    }

    /// Returns whether this function is exported.
    pub const fn is_exported(&self) -> bool {
        self.exported
    }
}
