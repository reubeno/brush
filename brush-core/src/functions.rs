//! Structures for managing function registrations and calls.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use brush_parser::ast;

/// An environment for defined, named functions.
#[derive(Clone, Default)]
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
#[derive(Clone)]
pub struct Registration {
    /// The definition of the function.
    pub(crate) definition: Arc<brush_parser::ast::FunctionDefinition>,
    /// Whether or not this function definition should be exported to children.
    exported: bool,
}

impl From<brush_parser::ast::FunctionDefinition> for Registration {
    fn from(definition: brush_parser::ast::FunctionDefinition) -> Self {
        Self {
            definition: Arc::new(definition),
            exported: false,
        }
    }
}

impl Registration {
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

/// Represents an active shell function call.
#[derive(Clone, Debug)]
pub struct FunctionCall {
    /// The name of the function invoked.
    pub function_name: String,
    /// The definition of the invoked function.
    pub function_definition: Arc<brush_parser::ast::FunctionDefinition>,
}

/// Encapsulates a function call stack.
#[derive(Clone, Debug, Default)]
pub struct CallStack {
    frames: VecDeque<FunctionCall>,
}

impl std::fmt::Display for CallStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return Ok(());
        }

        writeln!(f, "Function call stack (most recent first):")?;

        for (index, frame) in self.iter().enumerate() {
            writeln!(f, "  #{}| {}", index, frame.function_name)?;
        }

        Ok(())
    }
}

impl CallStack {
    /// Creates a new empty function call stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes the top from from the stack. If the stack is empty, does nothing and
    /// returns `None`; otherwise, returns the removed call frame.
    pub fn pop(&mut self) -> Option<FunctionCall> {
        self.frames.pop_front()
    }

    /// Pushes a new frame onto the stack.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being called.
    /// * `function_def` - The definition of the function being called.
    pub fn push(&mut self, name: impl Into<String>, function_def: &Arc<ast::FunctionDefinition>) {
        self.frames.push_front(FunctionCall {
            function_name: name.into(),
            function_definition: function_def.clone(),
        });
    }

    /// Returns the current depth of the function call stack.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Returns whether or not the function call stack is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Returns an iterator over the function call frames, starting from the most
    /// recent.
    pub fn iter(&self) -> impl Iterator<Item = &FunctionCall> {
        self.frames.iter()
    }
}
