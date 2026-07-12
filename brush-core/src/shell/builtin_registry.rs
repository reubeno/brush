//! Builtin command management for shell instances.

use std::any::TypeId;
use std::collections::HashMap;

use crate::{builtins, error, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Register a builtin to the shell's environment, replacing any existing
    /// registration with the same name.
    ///
    /// Also seeds the per-builtin state map with a default-constructed
    /// `B::State` (via the registration's `state_init` function), or with the
    /// custom state provided via [`Registration::with_state`], so that
    /// [`Self::builtin_state_of`] / [`Self::builtin_state_mut_of`] can
    /// always find an entry.
    ///
    /// Only accepts `Registration<SE, (), _>` — i.e. builtins whose
    /// `SharedState` is `()`. Use [`Self::register_shared`] for builtins
    /// that share state.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register_builtin<L>(
        &mut self,
        name: impl Into<String>,
        registration: builtins::Registration<SE, (), L>,
    ) {
        let key = name.into();
        let (stored, local_override, state_init) = registration.into_parts();
        self.builtins.insert(key.clone(), stored);
        match local_override {
            Some(state) => {
                self.builtin_states.insert(key, state);
            }
            None => {
                self.builtin_states.entry(key).or_insert_with(state_init);
            }
        }
    }

    /// Register a builtin only if no builtin with that name is already registered.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register_builtin_if_unset<L>(
        &mut self,
        name: impl Into<String>,
        registration: builtins::Registration<SE, (), L>,
    ) {
        let key = name.into();
        if self.builtins.contains_key(&key) {
            return;
        }
        self.register_builtin(key, registration);
    }

    /// Bulk-register builtins that share a single typed state value.
    ///
    /// Seeds `shared_states[TypeId::of::<T>()]` with the builder's value,
    /// then registers each builtin in the builder.
    ///
    /// # Arguments
    ///
    /// * `builder` - A [`SharedBuilder`](builtins::SharedBuilder) produced by chaining
    ///   `.builtin(name, reg)` calls.
    pub fn register_shared<T>(&mut self, builder: builtins::SharedBuilder<T, SE>)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.shared_states
            .insert(TypeId::of::<T>(), Box::new(builder.value));
        for (name, reg) in builder.builtins {
            let key = name;
            let (stored, local_override, state_init) = reg.into_parts();
            self.builtins.insert(key.clone(), stored);
            match local_override {
                Some(state) => {
                    self.builtin_states.insert(key, state);
                }
                None => {
                    self.builtin_states.entry(key).or_insert_with(state_init);
                }
            }
        }
    }

    /// Returns a borrowing handle for registering additional builtins against
    /// an **existing** shared state entry of type `T`.
    ///
    /// Each call to [`SharedHandle::builtin`] registers immediately.
    ///
    /// # Panics
    ///
    /// Methods on the returned handle panic if shared state for `T` has not
    /// been seeded (i.e. [`register_shared`](Self::register_shared) or
    /// [`set_shared`](Self::set_shared) has not been called for `T`).
    #[allow(clippy::missing_const_for_fn)]
    pub fn shared_handle<T>(&mut self) -> builtins::SharedHandle<'_, T, SE>
    where
        T: Clone + Send + Sync + 'static,
    {
        builtins::SharedHandle {
            shell: self,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Directly insert or replace a shared state value, keyed by `TypeId::of::<T>()`.
    ///
    /// This is intended for use outside of builtins (e.g. by embedders
    /// preparing a shell before handing it to user code). Builtins should
    /// prefer [`register_shared`](Self::register_shared) which atomically
    /// seeds shared state and registers builtins.
    pub fn set_shared<T: Clone + Send + Sync + 'static>(&mut self, state: T) {
        self.shared_states
            .insert(TypeId::of::<T>(), Box::new(state));
    }

    /// Returns a shared reference to the shared state of type `T`, or `None`
    /// if not registered.
    pub fn shared<T: 'static>(&self) -> Option<&T> {
        self.shared_states
            .get(&TypeId::of::<T>())
            .and_then(|s| (**s).as_any().downcast_ref::<T>())
    }

    /// Returns the raw shared-states map (for internal use by `SharedHandle`).
    pub(crate) fn shared_states(&self) -> &HashMap<TypeId, Box<dyn builtins::AnyState>> {
        &self.shared_states
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

    /// Retrieves a shared reference to the cross-builtin shared state of type `T`.
    ///
    /// Returns `Err` if no shared state of that type has been registered.
    pub fn get_shared<T: Clone + Send + Sync + 'static>(&self) -> Result<&T, error::Error> {
        self.shared_states
            .get(&TypeId::of::<T>())
            .and_then(|s| (**s).as_any().downcast_ref::<T>())
            .ok_or_else(|| {
                error::ErrorKind::SharedStateNotRegistered(std::any::type_name::<T>().to_string())
                    .into()
            })
    }
}
