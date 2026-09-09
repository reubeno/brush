//! Structures for managing function registrations and calls.

use std::{collections::HashMap, sync::Arc};

/// An environment for defined, named functions.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FunctionEnv {
    functions: HashMap<String, Registration>,
}

impl FunctionEnv {
    /// Tries to retrieve the registration for a function by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to retrieve.
    pub fn get(&self, name: &str) -> Option<&Registration> {
        self.functions.get(name)
    }

    /// Tries to retrieve a mutable reference to the registration for a
    /// function by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to retrieve.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Registration> {
        self.functions.get_mut(name)
    }

    /// Unregisters a function from the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to remove.
    pub fn remove(&mut self, name: &str) -> Option<Registration> {
        self.functions.remove(name)
    }

    /// Updates a function registration in this environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to update.
    /// * `registration` - The new registration for the function.
    pub fn update(&mut self, name: String, registration: Registration) {
        self.functions.insert(name, registration);
    }

    /// Clear all functions in this environment.
    pub fn clear(&mut self) {
        self.functions.clear();
    }

    /// Returns an iterator over the functions registered in this environment.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Registration)> {
        self.functions.iter()
    }
}

/// Encapsulates a registration for a defined function.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Registration {
    /// The parsed definition of the function.
    definition: Arc<brush_parser::ast::FunctionDefinition>,
    /// The source info for the function definition.
    source_info: crate::SourceInfo,
    /// Whether or not this function definition should be exported to children.
    exported: bool,
    /// Whether or not this function may be redefined or unset.
    #[cfg_attr(feature = "serde", serde(default))]
    readonly: bool,
    /// Whether or not this function inherits the DEBUG and RETURN traps. The attribute is
    /// recorded and displayed (`declare -ft`) but not yet honored by trap handling.
    #[cfg_attr(feature = "serde", serde(default))]
    traced: bool,
}

impl From<brush_parser::ast::FunctionDefinition> for Registration {
    fn from(definition: brush_parser::ast::FunctionDefinition) -> Self {
        Self {
            definition: Arc::new(definition),
            source_info: crate::SourceInfo::default(),
            exported: false,
            readonly: false,
            traced: false,
        }
    }
}

impl Registration {
    /// Creates a new function registration.
    ///
    /// # Arguments
    ///
    /// * `definition` - The function definition.
    /// * `source_info` - Source information for the function definition.
    pub fn new(
        definition: brush_parser::ast::FunctionDefinition,
        source_info: &crate::SourceInfo,
    ) -> Self {
        Self {
            definition: Arc::new(definition),
            source_info: source_info.clone(),
            exported: false,
            readonly: false,
            traced: false,
        }
    }

    /// Returns a reference to the function definition.
    pub fn definition(&self) -> &brush_parser::ast::FunctionDefinition {
        &self.definition
    }

    /// Returns a reference to the source info for the function definition.
    pub const fn source(&self) -> &crate::SourceInfo {
        &self.source_info
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

    /// Marks the function readonly: it can no longer be redefined or unset.
    pub const fn set_readonly(&mut self) {
        self.readonly = true;
    }

    /// Returns whether this function is readonly.
    pub const fn is_readonly(&self) -> bool {
        self.readonly
    }

    /// Enables tracing for the function.
    pub const fn enable_trace(&mut self) {
        self.traced = true;
    }

    /// Disables tracing for the function.
    pub const fn disable_trace(&mut self) {
        self.traced = false;
    }

    /// Returns whether tracing is enabled for this function.
    pub const fn is_trace_enabled(&self) -> bool {
        self.traced
    }

    /// Returns the canonical attribute flag string for this function, as displayed by
    /// `declare -F`: `f` followed by the attributes the function carries.
    pub fn attribute_flags(&self) -> String {
        let mut flags = String::from("f");
        if self.readonly {
            flags.push('r');
        }
        if self.traced {
            flags.push('t');
        }
        if self.exported {
            flags.push('x');
        }
        flags
    }
}
