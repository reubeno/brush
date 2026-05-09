//! Builtin command management for shell instances.

use std::collections::HashMap;

use crate::{builtins, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Register a builtin to the shell's environment, replacing any existing
    /// registration with the same name.
    ///
    /// Also seeds the per-builtin state map with a default-constructed
    /// `B::State` (via the registration's `state_init` function), so that
    /// [`Self::builtin_state_of`] / [`Self::builtin_state_mut_of`] can
    /// always find an entry.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register_builtin<S: Into<String>>(
        &mut self,
        name: S,
        registration: builtins::Registration<SE>,
    ) {
        let key = name.into();
        self.builtins.insert(key.clone(), registration.clone());
        self.builtin_states
            .entry(key)
            .or_insert_with(registration.state_init);
    }

    /// Register a builtin with an explicit initial state, replacing any
    /// existing registration and state with the same name.
    ///
    /// Use this when the builtin's default state is not the `Default::default()`
    /// value — for example, when the state needs to reference external data or
    /// have non-trivial initialization.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    /// * `state` - The initial state value.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register_builtin_with_state<S, T>(
        &mut self,
        name: S,
        registration: builtins::Registration<SE>,
        state: T,
    ) where
        S: Into<String>,
        T: Clone + Send + Sync + 'static,
    {
        let key = name.into();
        self.builtins.insert(key.clone(), registration);
        self.builtin_states.insert(key, Box::new(state));
    }

    /// Register a builtin only if no builtin with that name is already registered.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register_builtin_if_unset<S: Into<String>>(
        &mut self,
        name: S,
        registration: builtins::Registration<SE>,
    ) {
        let key = name.into();
        if self.builtins.contains_key(&key) {
            return;
        }
        self.register_builtin(key, registration);
    }

    /// Tries to retrieve a mutable reference to an existing builtin registration.
    /// Returns `None` if no such registration exists.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the builtin to lookup.
    pub fn builtin_mut(&mut self, name: &str) -> Option<&mut builtins::Registration<SE>> {
        self.builtins.get_mut(name)
    }

    /// Returns the registered builtins for the shell.
    pub const fn builtins(&self) -> &HashMap<String, builtins::Registration<SE>> {
        &self.builtins
    }

    // -- Typed state accessors (by Command type) --

    /// Returns a shared reference to the state of the named builtin, using
    /// `B::State` as the expected type.
    ///
    /// Returns `None` if no state has been registered for that name, or if the
    /// stored state is not of type `B::State`.
    pub fn builtin_state_of<B: builtins::Command>(&self, name: &str) -> Option<&B::State> {
        let state = self.builtin_states.get(name)?;
        (**state).as_any().downcast_ref::<B::State>()
    }

    /// Returns an exclusive reference to the state of the named builtin, using
    /// `B::State` as the expected type.
    ///
    /// Returns `None` if no state has been registered for that name, or if the
    /// stored state is not of type `B::State`.
    pub fn builtin_state_mut_of<B: builtins::Command>(
        &mut self,
        name: &str,
    ) -> Option<&mut B::State> {
        let state = self.builtin_states.get_mut(name)?;
        (**state).as_any_mut().downcast_mut::<B::State>()
    }

    // -- Raw typed state accessors (by concrete type) --

    /// Returns a shared reference to the state of the named builtin.
    ///
    /// Returns `None` if no state has been registered for that name, or if the
    /// stored state is not of the requested type `T`.
    pub fn builtin_state<T: 'static>(&self, name: &str) -> Option<&T> {
        let state = self.builtin_states.get(name)?;
        (**state).as_any().downcast_ref::<T>()
    }

    /// Returns an exclusive reference to the state of the named builtin.
    ///
    /// Returns `None` if no state has been registered for that name, or if the
    /// stored state is not of the requested type `T`.
    pub fn builtin_state_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        let state = self.builtin_states.get_mut(name)?;
        (**state).as_any_mut().downcast_mut::<T>()
    }
}
