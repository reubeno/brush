//! Builtin command management for shell instances.

use crate::{builtins, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the builtins registered with the shell.
    pub const fn builtins(&self) -> &builtins::Registry<SE> {
        &self.builtins
    }

    /// Returns the builtins registered with the shell, to register, enable or
    /// disable them.
    pub const fn builtins_mut(&mut self) -> &mut builtins::Registry<SE> {
        &mut self.builtins
    }

    /// Returns the builtin state of type `S` (see
    /// [`crate::ExecutionContext::state_mut`]), if a builtin has used it.
    pub fn builtin_state<S: 'static>(&self) -> Option<&S> {
        self.builtin_states.get()
    }

    /// Sets the builtin state of type `S` (see
    /// [`crate::ExecutionContext::state_mut`]), replacing any there was. A
    /// builtin then starts from this value rather than from `S::default()`, so
    /// the program embedding the shell can hand its builtins a value of its
    /// own: a handle it keeps a clone of, or settings it read elsewhere.
    pub fn set_builtin_state<S: Clone + Send + Sync + 'static>(&mut self, state: S) {
        self.builtin_states.set(state);
    }

    pub(crate) const fn builtin_states_mut(&mut self) -> &mut builtins::BuiltinStates {
        &mut self.builtin_states
    }
}
